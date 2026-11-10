"""Verify the production Flutter host survives Activity finish/reopen in a fresh package.

python3 test_driver/android/engine_lifecycle_test.py --device <adb-device>
Needs Flutter, a JDK, Android SDK/NDK 30.0.16248370, and an arm64/x64 device.
Only native DNS/TLS initialization is stubbed; engine, channels and leases use
production code. No Rust Hub, secure storage, pairing or ordinary app is opened.
"""

import argparse
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import time
import uuid


mobile = pathlib.Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--device", required=True)
parser.add_argument(
    "--sdk",
    default=os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT"),
)
args = parser.parse_args()
if not args.sdk:
    args.sdk = re.search(
        r"^sdk.dir=(.+)$",
        (mobile / "android/local.properties").read_text(),
        re.MULTILINE,
    )[1]
sdk = pathlib.Path(args.sdk)
adb = [str(sdk / "platform-tools/adb"), "-s", args.device]


def run(command, cwd=None):
    result = subprocess.run(
        list(map(str, command)), cwd=cwd, capture_output=True, text=True
    )
    if result.returncode:
        raise RuntimeError(result.stdout + result.stderr)
    return result.stdout


identifier = uuid.uuid4().hex
package = "fyi.omochi.leasefixture.x" + identifier
scheme = "omochi-engine-" + identifier
marker = "LeaseEngine" + identifier
abi = run(adb + ["shell", "getprop", "ro.product.cpu.abi"]).strip()
platform, triple = {
    "x86_64": ("android-x64", "x86_64-linux-android"),
    "arm64-v8a": ("android-arm64", "aarch64-linux-android"),
}[abi]
host = "darwin-x86_64" if sys.platform == "darwin" else "linux-x86_64"
clang = sdk / f"ndk/30.0.16248370/toolchains/llvm/prebuilt/{host}/bin/{triple}24-clang"
with tempfile.TemporaryDirectory(prefix="omochi-engine-lease-") as temporary:
    project = pathlib.Path(temporary) / "fixture"
    run(
        [
            "flutter",
            "create",
            "--empty",
            "--platforms",
            "android",
            "--project-name",
            "lease_fixture",
            project,
        ]
    )
    for name in [
        "kotlin/fyi/omochi/mobile/MainActivity.kt",
        "java/fyi/omochi/mobile/IdentityLeases.java",
    ]:
        destination = project / "android/app/src/main" / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(mobile / "android/app/src/main" / name, destination)
    manifest = (mobile / "android/app/src/main/AndroidManifest.xml").read_text()
    manifest = manifest.replace(
        'android:name=".MainActivity"', 'android:name="fyi.omochi.mobile.MainActivity"'
    )
    manifest = manifest.replace('android:scheme="omochi"', f'android:scheme="{scheme}"')
    manifest = manifest.replace(
        'android:label="Omochi"', 'android:label="Isolated engine lease test"'
    )
    (project / "android/app/src/main/AndroidManifest.xml").write_text(manifest)
    shutil.copy2(
        mobile / "android/app/src/debug/AndroidManifest.xml",
        project / "android/app/src/debug/AndroidManifest.xml",
    )
    gradle = project / "android/app/build.gradle.kts"
    gradle.write_text(
        re.sub(
            r'applicationId = "[^"]+"',
            f'applicationId = "{package}"',
            gradle.read_text(),
        )
    )
    pubspec = project / "pubspec.yaml"
    pubspec.write_text(
        pubspec.read_text().replace(
            "dependencies:\n", "dependencies:\n  app_links: ^7.0.0\n", 1
        )
    )
    shutil.copy2(
        mobile / "test_driver/android/engine_lifecycle.dart", project / "lib/main.dart"
    )
    stub = project / "init.c"
    stub.write_text(
        "void Java_fyi_omochi_mobile_MainActivity_initSharedHub(void *env, void *activity, void *context) {}\n"
    )
    native = project / "android/app/src/main/jniLibs" / abi
    native.mkdir(parents=True)
    run([clang, "-shared", "-fPIC", stub, "-o", native / "libomochi_native_bridge.so"])
    run(
        [
            "flutter",
            "build",
            "apk",
            "--debug",
            "--target-platform",
            platform,
            "--dart-define=LEASE_ENGINE_MARKER=" + marker,
        ],
        cwd=project,
    )
    try:
        run(
            adb
            + ["install", str(project / "build/app/outputs/flutter-apk/app-debug.apk")]
        )
        target = package + "/fyi.omochi.mobile.MainActivity"

        def launch():
            run(
                adb
                + [
                    "shell",
                    "am",
                    "start",
                    "-n",
                    target,
                    "-a",
                    "android.intent.action.MAIN",
                    "-c",
                    "android.intent.category.LAUNCHER",
                    "--ez",
                    "enable-impeller",
                    "false",
                ]
            )

        def logs():
            output = run(adb + ["logcat", "-d", "-s", "flutter:I", "*:S"])
            return "\n".join(line for line in output.splitlines() if marker in line)

        def wait_for(message):
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                output = logs()
                if message in output:
                    return output
                time.sleep(0.1)
            raise AssertionError(f"Missing {message!r}:\n{logs()}")

        def link(path, host="invite"):
            run(
                adb
                + [
                    "shell",
                    "am",
                    "start",
                    "-p",
                    package,
                    "-a",
                    "android.intent.action.VIEW",
                    "-c",
                    "android.intent.category.BROWSABLE",
                    "-d",
                    scheme + "://" + host + "/" + path,
                ]
            )

        link("cold", "owner")
        wait_for(marker + " owned")
        wait_for(marker + " link owner /cold")
        pid = run(adb + ["shell", "pidof", package]).strip()
        for host in ("invite", "share", "owner"):
            link("warm", host)
            wait_for(marker + " link " + host + " /warm")
        link("finish")
        wait_for(marker + " lifecycle detached")
        assert run(adb + ["shell", "pidof", package]).strip() == pid
        launch()
        link("check")
        output = wait_for(marker + " reacquired")
        assert run(adb + ["shell", "pidof", package]).strip() == pid
        assert output.count(marker + " main") == 1, output
        assert output.count(marker + " owned") == 1, output
        print(
            "PASS: Activity finish/reopen retains the same live Dart owner, lease and invitation/channel delivery"
        )
        print(output)
    finally:
        run(adb + ["uninstall", package])
