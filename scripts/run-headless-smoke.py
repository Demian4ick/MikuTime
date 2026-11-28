"""Private Xvfb test with explicitly owned D-Bus/AT-SPI services, no systemd changes."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time
from xml.sax.saxutils import escape
from smoke_fixture import seed_silent_data

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--debug', action='store_true')
parser.add_argument('--quick', action='store_true', help='Skip real one-minute focus/rest waits during test development')
parser.add_argument('--obsidian-only', action='store_true', help='Check only the native export-folder connection')
parser.add_argument('--panel-only', action='store_true', help='Check panel mode and the native CLI')
parser.add_argument('--categories-only', action='store_true', help='Create/delete categories using only a private profile')
parser.add_argument('--startup-only', action='store_true', help='Check state/dialogue readiness and both windows immediately after launch')
parser.add_argument('--appearance-only', action='store_true', help='Private wallpaper gallery, dark theme and reload checks')
parser.add_argument('--quit-only', choices=['main', 'companion'])
parser.add_argument('--quiet-exit', action='store_true')
args = parser.parse_args()
profile = 'debug' if args.debug else 'release'
if os.environ.get('MIKU_PRIVATE_SMOKE') != '1':
    raise SystemExit('Requires an explicitly isolated xvfb-run session')
data = root / '.local-test' / f'headless-ru-{time.time_ns()}'
data.mkdir(parents=True, mode=0o700)
seed_silent_data(data / 'data/dev.mikufocus.desktop')
if args.appearance_only:
    from appearance_smoke import seed
    seed(data / 'data/dev.mikufocus.desktop')
    home = data / 'home'; home.mkdir(mode=0o700)
    os.environ['HOME'] = str(home)
runtime = data / 'runtime'
runtime.mkdir(mode=0o700)
bus_config = data / 'bus.conf'
bus_config.write_text(f'''<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={escape(str(data / 'bus.sock'))}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow user="{os.getuid()}"/>
    <allow own="*"/>
    <allow send_destination="*"/>
    <allow receive_type="method_call"/><allow receive_type="method_return"/>
    <allow receive_type="error"/><allow receive_type="signal"/>
  </policy>
</busconfig>
''')
# No service directories: the test never auto-starts the user's portals, systemd,
# GVFS or desktop daemons. Only the explicitly started test registry owns this bus.
for key in ['WAYLAND_DISPLAY', 'HYPRLAND_INSTANCE_SIGNATURE', 'AT_SPI_BUS_ADDRESS',
            'DBUS_STARTER_ADDRESS', 'DBUS_STARTER_BUS_TYPE', 'GBM_BACKEND',
            '__GLX_VENDOR_LIBRARY_NAME', 'LIBVA_DRIVER_NAME', 'DRI_PRIME']:
    os.environ.pop(key, None)
os.environ.update({
    'GDK_BACKEND': 'x11', 'XDG_SESSION_TYPE': 'x11', 'XDG_CURRENT_DESKTOP': '',
    'GTK_USE_PORTAL': '0', 'GIO_USE_VFS': 'local', 'XDG_RUNTIME_DIR': str(runtime),
    'GSETTINGS_BACKEND': 'memory',
    'PULSE_PROP': 'application.name=miku-ui-smoke module-stream-restore.id=miku-ui-smoke',
    'XDG_DATA_HOME': str(data / 'data'), 'XDG_CONFIG_HOME': str(data / 'config'),
    'XDG_CACHE_HOME': str(data / 'cache'), 'WEBKIT_DISABLE_DMABUF_RENDERER': '1',
})

def stop_owned(process):
    if process is not None and process.poll() is None:
        process.terminate()
        try: process.wait(timeout=10)
        except subprocess.TimeoutExpired: process.kill(); process.wait()

print('isolated test directory', data, flush=True)
bus = registry = app = None
with (data / 'desktop.log').open('w') as log:
    try:
        bus = subprocess.Popen(['dbus-daemon', '--nofork', '--nopidfile', '--print-address=1', f'--config-file={bus_config}'], stdout=subprocess.PIPE, stderr=log, text=True)
        address = bus.stdout.readline().strip()
        if not address:
            raise RuntimeError(f'Private test bus failed; see {log.name}')
        os.environ['DBUS_SESSION_BUS_ADDRESS'] = address
        os.environ['AT_SPI_BUS_ADDRESS'] = address
        registry = subprocess.Popen(['/usr/lib/at-spi2-registryd'], stdout=log, stderr=log)
        for _ in range(50):
            result = subprocess.run(['gdbus', 'call', '--address', address, '--dest', 'org.freedesktop.DBus', '--object-path', '/org/freedesktop/DBus', '--method', 'org.freedesktop.DBus.NameHasOwner', 'org.a11y.atspi.Registry'], capture_output=True, text=True)
            if result.stdout.strip() == '(true,)': break
            if registry.poll() is not None:
                raise RuntimeError(f'Private accessibility registry failed; see {log.name}')
            time.sleep(.1)
        else: raise RuntimeError('Private accessibility registry did not become ready')

        import gi
        gi.require_version('Atspi', '2.0')
        from gi.repository import Atspi
        app = subprocess.Popen([str(root / f'src-tauri/target/{profile}/miku-focus')], stdout=log, stderr=log)
        print('isolated test pid', app.pid, flush=True)
        for _ in range(100):
            desktop = Atspi.get_desktop(0)
            if any(desktop.get_child_at_index(i).get_process_id() == app.pid for i in range(desktop.get_child_count())): break
            if app.poll() is not None:
                raise RuntimeError(f'Native app exited with {app.returncode}: {log.name}')
            time.sleep(.2)
        else: raise RuntimeError('Native app did not register on the private accessibility bus')
        time.sleep(1)
        command = ['python', str(root / 'scripts/desktop-smoke.py'), '--pid', str(app.pid), '--data', str(data / 'data/dev.mikufocus.desktop'), '--x11']
        if not args.quick: command.append('--full')
        if args.obsidian_only: command.append('--obsidian-only')
        if args.panel_only: command.append('--panel-only')
        if args.categories_only: command.append('--categories-only')
        if args.startup_only: command.append('--startup-only')
        if args.appearance_only: command.append('--appearance-only')
        if args.quit_only: command.extend(['--quit-only', args.quit_only])
        if args.quiet_exit: command.append('--quiet-exit')
        subprocess.run(command, cwd=root, check=True, timeout=300 if not args.quick else 180)
        code = app.wait(timeout=15)
        print(f'isolated {profile} exit code', code, flush=True)
        assert code == 0
        (data / 'exit.json').write_text(json.dumps({'exitCode': code, 'profile': profile}))
        if args.appearance_only:
            app = subprocess.Popen([str(root / f'src-tauri/target/{profile}/miku-focus')], stdout=log, stderr=log)
            time.sleep(2)
            subprocess.run(['python', str(root / 'scripts/desktop-smoke.py'), '--pid', str(app.pid), '--data', str(data / 'data/dev.mikufocus.desktop'), '--x11', '--appearance-reload'], cwd=root, check=True, timeout=120)
            assert app.wait(timeout=15) == 0
            print('appearance reload exit code 0', flush=True)
    finally:
        stop_owned(app)
        stop_owned(registry)
        stop_owned(bus)
