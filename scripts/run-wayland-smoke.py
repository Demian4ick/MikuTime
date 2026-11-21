"""Own one PID and private app-data; never select or close an existing user window."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import gi
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi
from smoke_fixture import seed_silent_data

p = argparse.ArgumentParser()
p.add_argument('--debug', action='store_true')
p.add_argument('--full', action='store_true')
p.add_argument('--panel-only', action='store_true')
p.add_argument('--pointer-helper', type=Path)
args = p.parse_args()
if not os.environ.get('WAYLAND_DISPLAY') or not os.environ.get('HYPRLAND_INSTANCE_SIGNATURE'):
    raise SystemExit('Requires an existing Hyprland/Wayland session; this is not an X11 test')
root = Path(__file__).resolve().parent.parent
profile = 'debug' if args.debug else 'release'
base = root / '.local-test' / f'wayland-{profile}-{time.time_ns()}'
base.mkdir(mode=0o700)
seed_silent_data(base / 'data/dev.mikufocus.desktop', with_export=True)
env = dict(os.environ, GDK_BACKEND='wayland', GSETTINGS_BACKEND='memory', XDG_DATA_HOME=str(base / 'data'), XDG_CONFIG_HOME=str(base / 'config'), XDG_CACHE_HOME=str(base / 'cache'), PULSE_PROP='application.name=miku-ui-smoke module-stream-restore.id=miku-ui-smoke')
print('Test directory:', base, flush=True)
with (base / 'desktop.log').open('w') as log:
    app = subprocess.Popen([str(root / f'src-tauri/target/{profile}/miku-focus')], env=env, stdout=log, stderr=log)
    print('Owned test PID:', app.pid, flush=True)
    try:
        for _ in range(100):
            if app.poll() is not None: raise RuntimeError(f'App exited: {app.returncode}; see {log.name}')
            desktop = Atspi.get_desktop(0)
            if any(desktop.get_child_at_index(i).get_process_id() == app.pid for i in range(desktop.get_child_count())): break
            time.sleep(.2)
        else: raise RuntimeError('Owned app did not register on AT-SPI')
        time.sleep(1)
        command = [sys.executable, str(root / 'scripts/desktop-smoke.py'), '--pid', str(app.pid), '--data', str(base / 'data/dev.mikufocus.desktop')]
        if args.full: command.append('--full')
        if args.panel_only: command.append('--panel-only')
        if args.pointer_helper: command.extend(['--pointer-helper', str(args.pointer_helper.resolve())])
        subprocess.run(command, cwd=root, check=True, timeout=300 if args.full else 150)
        code = app.wait(timeout=15)
        (base / 'exit.json').write_text(json.dumps({'exitCode': code, 'pid': app.pid, 'profile': profile}))
        assert code == 0, code
        print(f'Wayland {profile} exit code:', code, flush=True)
    finally:
        if app.poll() is None:
            app.terminate()
            try: app.wait(timeout=10)
            except subprocess.TimeoutExpired: app.kill(); app.wait()
