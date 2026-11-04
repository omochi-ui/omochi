"""Run exact Android lease code on a JVM: python3 test_driver/android/identity_lease_test.py."""
import pathlib
import subprocess
import tempfile


mobile = pathlib.Path(__file__).resolve().parents[2]
source = mobile / "android/app/src/main/java/fyi/omochi/mobile/IdentityLeases.java"
harness = """
package fyi.omochi.mobile;
import java.io.*;
public class LeaseTest {
    public static void main(String[] args) throws Exception {
        File directory = new File(args[0]);
        String namespace = args[1];
        String token = IdentityLeases.acquire(directory, namespace);
        // A second engine must not open and close another channel for our file.
        for (int i = 0; i < 3; i++) {
            try {
                IdentityLeases.acquire(directory, namespace);
                throw new AssertionError("duplicate engine acquired identity");
            } catch (IOException expected) { }
        }
        String other = IdentityLeases.acquire(directory, namespace + ".other");
        IdentityLeases.release(other);
        IdentityLeases.release(other);
        IdentityLeases.release("unknown-token");
        System.out.println("acquired");
        System.out.flush();
        System.in.read();
        IdentityLeases.release(token);
        IdentityLeases.release(token);
        String retry = IdentityLeases.acquire(directory, namespace);
        IdentityLeases.release(retry);
    }
}
"""
with tempfile.TemporaryDirectory(prefix="omochi-android-lease-") as temporary:
    directory = pathlib.Path(temporary)
    test = directory / "LeaseTest.java"
    test.write_text(harness)
    subprocess.run(["javac", "-d", str(directory), str(source), str(test)], check=True)
    command = ["java", "-cp", str(directory), "fyi.omochi.mobile.LeaseTest", str(directory)]
    for crash in (False, True):
        with subprocess.Popen(command + ["omochi.test.owner"], stdin=subprocess.PIPE,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) as owner:
            assert owner.stdout.readline().strip() == "acquired"
            path = next((directory / "identity-leases").glob("*.lock"))
            inode = path.stat().st_ino
            blocked = subprocess.run(command + ["omochi.test.owner"], input="\n",
                                     capture_output=True, text=True, timeout=10)
            assert blocked.returncode != 0, "second process acquired identity"
            separate = subprocess.run(command + ["omochi.test.independent"], input="\n",
                                      capture_output=True, text=True, timeout=10)
            assert separate.returncode == 0, separate.stderr
            if crash:
                owner.kill()
                owner.wait(timeout=10)
            else:
                owner.communicate("\n", timeout=10)
                assert owner.returncode == 0
            assert path.stat().st_ino == inode, "lease file was replaced"
        retry = subprocess.run(command + ["omochi.test.owner"], input="\n",
                               capture_output=True, text=True, timeout=10)
        assert retry.returncode == 0, retry.stderr
print("PASS: engine/process exclusion, independent namespaces, idempotence, normal/crash reacquire")
