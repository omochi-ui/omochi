"""Windows CI: archive installation must leave default runtime ancestry private."""
import os
from pathlib import Path
import queue
import shutil
import subprocess
import tempfile
import threading
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[1]


def main():
    assert os.name == 'nt', 'run this installer check on Windows'
    with tempfile.TemporaryDirectory(prefix='omochi-install-') as directory:
        stage = Path(directory)
        (stage / 'bin').mkdir()
        for name in ('omochi-ui.exe', 'omochi-utils.exe'):
            shutil.copy2(ROOT / 'crates/target/debug' / name, stage / 'bin' / name)
        shutil.copy2(ROOT / 'scripts/install-native.ps1', stage / 'install.ps1')
        # Isolate install destinations while the native runtime still resolves the
        # real LocalAppData known folder, exactly as it does for installed users.
        env = {**os.environ, 'LOCALAPPDATA': str(stage / 'appdata')}
        env.pop('OMOCHI_RUNTIME_DIR', None)
        command = ['pwsh', '-NoProfile', '-NonInteractive', '-File', str(stage / 'install.ps1')]
        subprocess.run(command, env=env, check=True)
        prefix = stage / 'appdata/Programs/Omochi/bin'
        assert not (stage / 'appdata/Omochi').exists(), 'installer precreated managed ancestry'
        subprocess.run([str(prefix / 'omochi-ui.exe'), '--version'], check=True)
        custom = stage / 'custom prefix'
        subprocess.run([*command, '-Prefix', str(custom)], env=env, check=True)
        subprocess.run([str(custom / 'omochi-utils.exe'), '--version'], check=True)
        child = subprocess.Popen([str(prefix / 'omochi-ui.exe'), '--config-dir',
                                  str(stage / 'state'), 'hub', '--offline', '--port', '0'],
                                 env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                                 text=True)
        lines = queue.Queue()
        def read_ready():
            for line in child.stderr:
                if line.startswith('[omochi-ui] dashboard '):
                    lines.put(line)
                    return
            lines.put(None)
        threading.Thread(target=read_ready, daemon=True).start()
        try:
            line = lines.get(timeout=30)
            assert line is not None, 'hub exited before default runtime became ready'
            with urlopen(line.split('dashboard ', 1)[1].strip(), timeout=5) as response:
                assert response.status == 200
        finally:
            child.terminate()
            child.wait(timeout=10)
    print('Default and custom Windows installation and default hub runtime passed')


if __name__ == '__main__':
    main()
