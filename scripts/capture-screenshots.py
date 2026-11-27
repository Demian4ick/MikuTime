"""Capture a real Linux binary on private Xvfb, with generated demo data only.

Example (no user windows, sound, settings or history are touched):
  TMPDIR=/tmp/opencode xvfb-run -a -s '-screen 0 1280x1024x24 -nolisten tcp' \
    env MIKU_PRIVATE_SCREENSHOTS=1 python scripts/capture-screenshots.py --binary /path/to/miku-focus
Images and provenance stay in .local-test for review before publishing.
"""
import argparse
from datetime import datetime, timedelta
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parent.parent


def demo_state(now):
    """Fictional sessions; never read the user's profile or import any of its assets."""
    categories = [
        ('unity', 'Юнити', '#57b69c'), ('blender', 'Блендер', '#dfab72'),
        ('programming', 'Программирование', '#55aebe'),
        ('reading', 'Чтение', '#bd95bd'), ('other', 'Другое', '#8ca9b4'),
    ]
    sessions, breaks = [], []
    for days_ago in range(29, -1, -1):
        if days_ago % 7 == 4:
            continue
        day = now.replace(hour=8, minute=0, second=0, microsecond=0) - timedelta(days=days_ago)
        for slot in range(2 + days_ago % 5):
            start = int((day + timedelta(minutes=slot * 40)).timestamp() * 1000)
            end = start + (20 + (days_ago + slot) % 3 * 5) * 60000
            if end > int(now.timestamp() * 1000) - 60000:
                continue
            sessions.append({
                'id': f'demo-{days_ago}-{slot}', 'categoryId': categories[(days_ago + slot) % 4][0],
                'startedAt': start, 'endedAt': end, 'segments': [{'startMs': start, 'endMs': end}],
                'extensions': 1 if end - start == 30 * 60000 else 0, 'completed': True,
            })
            breaks.append({'startMs': end, 'endMs': end + 5 * 60000})
    catalog = json.loads((ROOT / 'src/lib/sound-catalog.json').read_text())
    voices = json.loads((ROOT / 'src/lib/voice-catalog.json').read_text())['voices']
    return {
        'schemaVersion': 1, 'savedAt': 0,
        'settings': {
            'volume': 0, 'sfxVolume': 0, 'quietMode': False, 'textSpeed': 0,
            'dailyGoalMinutes': 120, 'voices': {v['id']: v['file'] for v in voices},
            'sfxBindings': catalog['defaults'], 'soundDefaultsVersion': 2,
            'companionSize': [240, 260],
        },
        'timer': {'status': 'Idle', 'phase': 'Focus', 'remainingMs': 1500000, 'deadlineMs': None,
                  'segmentStartedAt': None, 'session': None, 'cycle': 0},
        'categories': [{'id': id, 'name': name, 'color': color, 'archived': False}
                       for id, name, color in categories],
        'selectedCategory': 'programming', 'sessions': sessions, 'breaks': breaks,
    }


def stop_owned(process):
    if process is not None and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    args = parser.parse_args()
    if os.environ.get('MIKU_PRIVATE_SCREENSHOTS') != '1' or not os.environ.get('DISPLAY'):
        parser.error('Run in an explicitly isolated xvfb-run session, never on the user display')
    binary = args.binary.resolve(strict=True)
    base = ROOT / '.local-test' / f'screenshots-{time.time_ns()}'
    base.mkdir(parents=True, mode=0o700)
    data = base / 'data/dev.mikufocus.desktop'
    data.mkdir(parents=True, mode=0o700)
    fixture = demo_state(datetime.now().astimezone())
    (data / 'state.json').write_text(json.dumps(fixture, ensure_ascii=False))
    runtime = base / 'runtime'
    runtime.mkdir(mode=0o700)
    images = base / 'images'
    images.mkdir()
    bus_config = base / 'bus.conf'
    bus_config.write_text(f'''<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig><type>session</type><listen>unix:path={escape(str(base / 'bus.sock'))}</listen>
<auth>EXTERNAL</auth><policy context="default"><allow user="{os.getuid()}"/>
<allow own="*"/><allow send_destination="*"/><allow receive_type="method_call"/>
<allow receive_type="method_return"/><allow receive_type="error"/><allow receive_type="signal"/>
</policy></busconfig>''')
    for key in ['WAYLAND_DISPLAY', 'HYPRLAND_INSTANCE_SIGNATURE', 'AT_SPI_BUS_ADDRESS',
                'DBUS_STARTER_ADDRESS', 'DBUS_STARTER_BUS_TYPE', 'GBM_BACKEND',
                '__GLX_VENDOR_LIBRARY_NAME', 'LIBVA_DRIVER_NAME', 'DRI_PRIME']:
        os.environ.pop(key, None)
    os.environ.update({
        'GDK_BACKEND': 'x11', 'XDG_SESSION_TYPE': 'x11', 'XDG_CURRENT_DESKTOP': '',
        'GTK_USE_PORTAL': '0', 'GIO_USE_VFS': 'local', 'GSETTINGS_BACKEND': 'memory',
        'XDG_RUNTIME_DIR': str(runtime), 'XDG_DATA_HOME': str(base / 'data'),
        'XDG_CONFIG_HOME': str(base / 'config'), 'XDG_CACHE_HOME': str(base / 'cache'),
        'WEBKIT_DISABLE_DMABUF_RENDERER': '1',
        'PULSE_PROP': 'application.name=miku-screenshots module-stream-restore.id=miku-screenshots',
    })
    bus = registry = app = None
    shots = []
    print('Private capture directory:', base, flush=True)
    with (base / 'desktop.log').open('w') as log:
        try:
            bus = subprocess.Popen(['dbus-daemon', '--nofork', '--nopidfile', '--print-address=1',
                                    f'--config-file={bus_config}'], stdout=subprocess.PIPE, stderr=log, text=True)
            address = bus.stdout.readline().strip()
            assert address, 'Private bus did not start'
            os.environ['DBUS_SESSION_BUS_ADDRESS'] = address
            os.environ['AT_SPI_BUS_ADDRESS'] = address
            registry = subprocess.Popen(['/usr/lib/at-spi2-registryd'], stdout=log, stderr=log)
            for _ in range(50):
                result = subprocess.run(['gdbus', 'call', '--address', address, '--dest', 'org.freedesktop.DBus',
                                         '--object-path', '/org/freedesktop/DBus', '--method',
                                         'org.freedesktop.DBus.NameHasOwner', 'org.a11y.atspi.Registry'],
                                        capture_output=True, text=True)
                if result.stdout.strip() == '(true,)':
                    break
                time.sleep(.1)
            else:
                raise RuntimeError('Private accessibility bus did not start')
            import gi
            gi.require_version('Atspi', '2.0')
            gi.require_version('Gdk', '3.0')
            gi.require_version('GdkX11', '3.0')
            from gi.repository import Atspi, Gdk, GdkX11, GLib
            app = subprocess.Popen([str(binary)], stdout=log, stderr=log)
            owned = None
            for _ in range(150):
                desktop = Atspi.get_desktop(0)
                owned = next((desktop.get_child_at_index(i) for i in range(desktop.get_child_count())
                              if desktop.get_child_at_index(i).get_process_id() == app.pid), None)
                if owned is not None:
                    break
                assert app.poll() is None, 'Owned application exited during startup'
                time.sleep(.1)
            assert owned is not None, 'Owned application not found on the private bus'

            def find(obj, name):
                try:
                    matches = name(obj) if callable(name) else obj.get_name() == name
                    if matches and obj.get_action_iface() is not None:
                        return obj
                    for index in range(obj.get_child_count()):
                        child = obj.get_child_at_index(index)
                        if child is not None:
                            match = find(child, name)
                            if match is not None:
                                return match
                except GLib.Error:
                    pass
                return None

            def click(name):
                for _ in range(60):
                    button = find(owned, name)
                    if button is not None and button.get_action_iface().do_action(0):
                        time.sleep(.3)
                        return
                    owned.clear_cache()
                    time.sleep(.1)
                raise AssertionError(f'Owned control not found: {name}')

            def window(title):
                return subprocess.check_output(['xdotool', 'search', '--all', '--onlyvisible', '--pid', str(app.pid),
                                                 '--name', title], text=True).splitlines()[0]

            main_window = window('^Miku Focus$')
            subprocess.run(['xdotool', 'windowsize', main_window, '1000', '760'], check=True)
            subprocess.run(['xdotool', 'windowmove', main_window, '40', '40'], check=True)
            time.sleep(1)

            def capture(name, title='^Miku Focus$'):
                wid = window(title)
                subprocess.run(['xdotool', 'windowfocus', '--sync', wid], check=True)
                # Clear keyboard focus rings using a real click on the owned non-interactive header.
                if title == '^Miku Focus$':
                    subprocess.run(['xdotool', 'mousemove', '--window', wid, '490', '35', 'click', '1'], check=True)
                subprocess.run(['xdotool', 'mousemove', '1250', '990'], check=True)
                time.sleep(.5)
                geometry = dict(line.split('=', 1) for line in subprocess.check_output(
                    ['xdotool', 'getwindowgeometry', '--shell', wid], text=True).splitlines())
                x, y, width, height = [int(geometry[key]) for key in ['X', 'Y', 'WIDTH', 'HEIGHT']]
                Gdk.Display.get_default().sync()
                if title == '^Miku Focus$':
                    image = Gdk.pixbuf_get_from_window(Gdk.get_default_root_window(), x, y, width, height)
                else:
                    # Read the owned ARGB surface directly: no compositor or fake backdrop is needed.
                    surface = GdkX11.X11Window.foreign_new_for_display(Gdk.Display.get_default(), int(wid))
                    image = Gdk.pixbuf_get_from_window(surface, 0, 0, width, height)
                assert image is not None
                output = images / f'{name}.png'
                image.savev(str(output), 'png', [], [])
                shots.append({'name': output.name, 'width': width, 'height': height,
                              'sha256': hashlib.sha256(output.read_bytes()).hexdigest()})
                print('Captured:', name, width, height, flush=True)

            capture('focus')
            click('✧ Дневник занятий'); click('Статистика'); capture('statistics')
            click('Календарь')
            last_day = datetime.fromtimestamp(fixture['sessions'][-1]['startedAt'] / 1000).day
            click(lambda obj: obj.get_name().split()[:1] == [str(last_day)])
            capture('calendar')
            click('Настройки'); click('Звуки'); click('Голоса (21)'); capture('voices')
            click('♡ Комната фокуса'); click('▶ Начать работу'); click('Помощник ↗')
            capture('companion', '^Miku Focus — помощник$')
            click('Мику: клик — открыть, перетаскивание — переместить')
            click('Сохранить и выйти')
            assert app.wait(timeout=15) == 0
            report = {'binarySha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'backend': 'Linux/X11/Xvfb',
                      'demoDataOnly': True, 'userWindowsTouched': False, 'exitCode': 0,
                      'demoSessions': len(fixture['sessions']), 'images': shots}
            (base / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2))
            print('Capture completed:', base / 'report.json', flush=True)
        finally:
            stop_owned(app)
            stop_owned(registry)
            stop_owned(bus)


if __name__ == '__main__':
    main()
