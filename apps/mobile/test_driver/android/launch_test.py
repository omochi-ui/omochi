"""Test real Android task routing and leases in a disposable package, never the app.

python3 test_driver/android/launch_test.py --device <adb-device> [--sdk <android-sdk>]
Requires a JDK and Android SDK build-tools/platforms. No Flutter/Rust build needed.
"""
import argparse
import os
import pathlib
import re
import subprocess
import tempfile
import time
import uuid
import xml.etree.ElementTree as ET
import zipfile


mobile = pathlib.Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--device", required=True)
parser.add_argument("--sdk", default=os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT"))
args = parser.parse_args()
if not args.sdk:
    properties = (mobile / "android/local.properties").read_text()
    args.sdk = re.search(r"^sdk.dir=(.+)$", properties, re.MULTILINE)[1]
sdk = pathlib.Path(args.sdk)


def latest(directory):
    return max(directory.iterdir(), key=lambda path: tuple(map(int, re.findall(r"\d+", path.name))))


tools = latest(sdk / "build-tools")
android = latest(sdk / "platforms") / "android.jar"
package = "fyi.omochi.leasefixture.x" + uuid.uuid4().hex
tag = "OmochiLease" + uuid.uuid4().hex[:8]
scheme = "omochi-lease-" + uuid.uuid4().hex
namespace = "omochi.test.fixture"
adb = [str(sdk / "platform-tools/adb"), "-s", args.device]


def run(command):
    return subprocess.run(list(map(str, command)), check=True, capture_output=True, text=True).stdout


def logs():
    return run(adb + ["logcat", "-d", "-s", tag + ":I", "*:S"])


def wait_for(text):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        output = logs()
        if text in output:
            return output
        time.sleep(0.1)
    raise AssertionError(f"Missing {text!r} in fixture logs:\n{logs()}")


manifest = ET.parse(mobile / "android/app/src/main/AndroidManifest.xml")
activity = manifest.find("application/activity")
attribute = "{http://schemas.android.com/apk/res/android}"
mode = activity.attrib[attribute + "launchMode"]
affinity = activity.attrib[attribute + "taskAffinity"]
with tempfile.TemporaryDirectory(prefix="omochi-android-launch-") as temporary:
    directory = pathlib.Path(temporary)
    (directory / "classes").mkdir()
    (directory / "dex").mkdir()
    (directory / "AndroidManifest.xml").write_text(f"""
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="{package}">
  <uses-sdk android:minSdkVersion="24" android:targetSdkVersion="36" />
  <application android:label="Omochi isolated lease test" android:theme="@android:style/Theme.Material.Light">
    <activity android:name="fyi.omochi.mobile.LeaseTarget" android:exported="true"
        android:launchMode="{mode}" android:taskAffinity="{affinity}">
      <intent-filter><action android:name="android.intent.action.MAIN"/>
        <category android:name="android.intent.category.LAUNCHER"/></intent-filter>
      <intent-filter><action android:name="android.intent.action.VIEW"/>
        <category android:name="android.intent.category.DEFAULT"/>
        <category android:name="android.intent.category.BROWSABLE"/>
        <data android:scheme="{scheme}" android:host="invite"/></intent-filter>
    </activity>
    <activity android:name="fyi.omochi.mobile.LeaseCaller" android:exported="true" android:taskAffinity="{package}.browser"/>
    <activity android:name="fyi.omochi.mobile.LeaseContender" android:exported="true" android:process=":contender"/>
  </application>
</manifest>""")
    sources = {
        "LeaseTarget": f"""
package fyi.omochi.mobile;
public class LeaseTarget extends android.app.Activity {{
  protected void onCreate(android.os.Bundle state) {{
    super.onCreate(state);
    android.util.Log.i("{tag}", "CREATE " + getTaskId() + " " + System.identityHashCode(this));
    try {{
      IdentityLeases.acquire(getFilesDir(), "{namespace}");
      try {{ IdentityLeases.acquire(getFilesDir(), "{namespace}"); throw new AssertionError("duplicate engine"); }}
      catch (java.io.IOException expected) {{ }}
      android.util.Log.i("{tag}", "OWNED");
    }} catch (Exception error) {{ throw new RuntimeException(error); }}
  }}
  protected void onNewIntent(android.content.Intent intent) {{
    super.onNewIntent(intent);
    android.util.Log.i("{tag}", "INTENT " + getTaskId() + " " + System.identityHashCode(this));
  }}
}}""",
        "LeaseCaller": f"""
package fyi.omochi.mobile;
public class LeaseCaller extends android.app.Activity {{
  protected void onCreate(android.os.Bundle state) {{
    super.onCreate(state);
    android.content.Intent intent = new android.content.Intent(android.content.Intent.ACTION_VIEW,
        android.net.Uri.parse("{scheme}://invite/synthetic"));
    intent.addCategory(android.content.Intent.CATEGORY_BROWSABLE);
    // Deliberately no flags: ordinary browser Activity -> external invitation.
    startActivity(intent);
  }}
}}""",
        "LeaseContender": f"""
package fyi.omochi.mobile;
public class LeaseContender extends android.app.Activity {{
  protected void onCreate(android.os.Bundle state) {{
    super.onCreate(state);
    try {{
      try {{ IdentityLeases.acquire(getFilesDir(), "{namespace}"); throw new AssertionError("second process"); }}
      catch (java.io.IOException expected) {{ }}
      String token = IdentityLeases.acquire(getFilesDir(), "{namespace}.other");
      IdentityLeases.release(token);
      IdentityLeases.release(token);
      IdentityLeases.release(IdentityLeases.acquire(getFilesDir(), "{namespace}.other"));
      android.util.Log.i("{tag}", "CONTENTION PASS");
    }} catch (Exception error) {{ throw new RuntimeException(error); }}
    finish();
  }}
}}""",
    }
    files = []
    for name, source in sources.items():
        path = directory / (name + ".java")
        path.write_text(source)
        files.append(path)
    files.append(mobile / "android/app/src/main/java/fyi/omochi/mobile/IdentityLeases.java")
    run(["javac", "--release", "8", "-cp", android, "-d", directory / "classes", *files])
    run([tools / "d8", "--min-api", "24", "--lib", android, "--output", directory / "dex",
         *sorted((directory / "classes").rglob("*.class"))])
    unsigned = directory / "unsigned.apk"
    run([tools / "aapt2", "link", "-I", android, "--manifest", directory / "AndroidManifest.xml", "-o", unsigned])
    with zipfile.ZipFile(unsigned, "a") as archive:
        archive.write(directory / "dex/classes.dex", "classes.dex")
    apk = directory / "fixture.apk"
    run([tools / "zipalign", "4", unsigned, apk])
    run(["keytool", "-genkeypair", "-keystore", directory / "key.jks", "-storepass", "fixturepass",
         "-keypass", "fixturepass", "-alias", "fixture", "-keyalg", "RSA", "-dname", "CN=Isolated test", "-validity", "1"])
    run([tools / "apksigner", "sign", "--ks", directory / "key.jks", "--ks-pass", "pass:fixturepass", apk])
    try:
        run(adb + ["install", str(apk)])
        target = package + "/fyi.omochi.mobile.LeaseTarget"
        run(adb + ["shell", "am", "start", "-n", target, "-a", "android.intent.action.MAIN", "-c", "android.intent.category.LAUNCHER"])
        wait_for("OWNED")
        run(adb + ["shell", "am", "start", "-n", package + "/fyi.omochi.mobile.LeaseContender"])
        wait_for("CONTENTION PASS")
        run(adb + ["shell", "am", "start", "-n", package + "/fyi.omochi.mobile.LeaseCaller"])
        output = wait_for("INTENT")
        created = re.findall(r"CREATE (\d+ \d+)", output)
        delivered = re.findall(r"INTENT (\d+ \d+)", output)
        assert len(created) == 1 and delivered == created, output
        run(adb + ["shell", "am", "force-stop", package])
        run(adb + ["shell", "am", "start", "-n", target])
        deadline = time.monotonic() + 15
        while logs().count("OWNED") != 2 and time.monotonic() < deadline:
            time.sleep(0.1)
        assert logs().count("OWNED") == 2, logs()
        print("PASS: browser no-flags intent reuses launcher Activity; Android engine/process exclusion, namespace isolation, idempotence and crash recovery")
        print(output)
    finally:
        run(adb + ["uninstall", package])
