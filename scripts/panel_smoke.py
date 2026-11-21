"""Owned-PID panel checks; never edit/restart the user's Waybar configuration/process."""
import json
import ast
import os
from pathlib import Path
import select
import signal
import subprocess
import time
from xml.sax.saxutils import escape


class Frames:
    def __init__(self, binary, env):
        self.process = subprocess.Popen([str(binary), '--waybar'], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0)
        self.buffer = b''
        self.seen = []

    def next(self, timeout=4):
        deadline = time.monotonic() + timeout
        while b'\n' not in self.buffer:
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([self.process.stdout], [], [], remaining)[0]: return None
            chunk = os.read(self.process.stdout.fileno(), 8192)
            if not chunk: raise AssertionError('Owned Waybar client ended unexpectedly')
            self.buffer += chunk
        line, self.buffer = self.buffer.split(b'\n', 1)
        frame = json.loads(line)
        self.seen.append(frame)
        assert len(self.seen) <= 1024, 'Panel must not flood updates'
        return frame

    def until(self, predicate, timeout=5):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            frame = self.next(deadline - time.monotonic())
            if frame is None: break
            if predicate(frame): return frame
        raise AssertionError('Expected Rust panel frame was not received')

    def stop(self):
        if self.process.poll() is None:
            self.process.terminate()
            self.process.wait(timeout=5)


def run(args, app, *, click, find, keys, state, persisted, check, client, all_text, settings_duration):
    import gi
    gi.require_version('Atspi', '2.0')
    from gi.repository import Atspi, GLib
    assert '.local-test' in args.data.resolve().parts
    binary = Path(f'/proc/{args.pid}/exe').resolve()
    env = dict(os.environ, XDG_DATA_HOME=str(args.data.parent), GSETTINGS_BACKEND='memory')
    frames = bar = bar_log = bar_bus = None

    def set_mode(panel):
        click('Настройки'); click('Система')
        control = find(app, lambda o: o.get_name() == 'Режим сворачивания')
        assert control is not None
        keys(control, 'End' if panel else 'Home', 'Return', 'Escape')
        assert persisted(lambda s: s['settings']['backgroundMode'] == ('Panel' if panel else 'Companion'))

    def show():
        subprocess.run([str(binary), '--show-main'], env=env, check=True, timeout=8)
        time.sleep(.3)

    def hidden():
        if args.x11:
            result = subprocess.run(['xdotool', 'search', '--all', '--onlyvisible', '--pid', str(args.pid), '--name', '^Miku Focus'], capture_output=True, text=True)
            return result.returncode == 1 and not result.stdout.strip()
        clients = json.loads(subprocess.check_output(['hyprctl', '-j', 'clients'], text=True))
        return not any(c['pid'] == args.pid for c in clients)

    def bar_text(expected):
        deadline = time.monotonic() + 5
        actual = ''
        while time.monotonic() < deadline:
            context = GLib.MainContext.default()
            while context.pending(): context.iteration(False)
            desktop = Atspi.get_desktop(0)
            owned = next((desktop.get_child_at_index(i) for i in range(desktop.get_child_count()) if desktop.get_child_at_index(i).get_process_id() == bar.pid), None)
            try:
                if owned:
                    owned.clear_cache()
                    actual = all_text(owned)
                    if expected in actual: return owned
            except GLib.Error: pass
            time.sleep(.1)
        (args.data / 'private-waybar-failure.json').write_text(json.dumps({'expected': expected, 'actual': actual, 'exitCode': bar.poll()}, ensure_ascii=False))
        raise AssertionError(f'Owned real Waybar did not display {expected!r}; actual={actual!r}, exit={bar.poll()}')

    try:
        before = state()
        check('legacy settings default to the companion', before['settings']['backgroundMode'] == 'Companion')
        set_mode(True)
        check('panel preference autosaves without changing timer/history', state()['timer'] == before['timer'] and state()['sessions'] == before['sessions'])
        click('Настройка модуля панели')
        config_control = find(app, lambda o: o.get_name() == 'Конфигурация модуля панели')
        assert config_control is not None
        config = json.loads(Atspi.Text.get_text(config_control, 0, -1))
        check('settings provide a streaming Waybar module and an open-window command', '--waybar' in config['custom/miku-focus']['exec'] and '--show-main' in config['custom/miku-focus']['on-click'] and 'interval' not in config['custom/miku-focus'])
        settings_duration(1)
        frames = Frames(binary, env)
        frames.until(lambda f: f['class'] == 'idle' and f['text'] == '01:00')
        check('the native CLI receives the exact idle Rust dial', True)
        if not args.x11:
            monitors = json.loads(subprocess.check_output(['hyprctl', '-j', 'monitors'], text=True))
            monitor_id = client()['monitor']
            output = next(m['name'] for m in monitors if m['id'] == monitor_id)
            config.update({'name': 'miku-panel-native-smoke', 'output': output, 'position': 'bottom', 'layer': 'overlay', 'exclusive': False, 'width': 160, 'height': 28, 'margin-bottom': 8, 'modules-center': ['custom/miku-focus']})
            config_path = args.data / 'private-waybar.json'
            css_path = args.data / 'private-waybar.css'
            config_path.write_text(json.dumps(config))
            css_path.write_text('window#waybar { background: #102a2e; color: #eafff8; border-radius: 0; font-family: monospace; font-size: 14px; } #custom-miku-focus { padding: 0 8px; } #custom-miku-focus.decision { color: #ffa8bc; }')
            bar_log = (args.data / 'private-waybar.log').open('w')
            bar_env = dict(env, GDK_BACKEND='wayland', XDG_CONFIG_HOME=str(args.data.parent.parent / 'config'), XDG_CACHE_HOME=str(args.data.parent.parent / 'cache'))
            # A private session bus prevents any GApplication forwarding or changes to
            # the user's Waybar/portal services. Only accessibility uses the existing bus.
            a11y = bar_env.get('AT_SPI_BUS_ADDRESS')
            if not a11y:
                a11y = ast.literal_eval(subprocess.check_output(['gdbus', 'call', '--session', '--dest', 'org.a11y.Bus', '--object-path', '/org/a11y/bus', '--method', 'org.a11y.Bus.GetAddress'], text=True))[0]
            bus_path = args.data.parent.parent / 'waybar-bus.conf'
            socket_path = args.data.parent.parent / 'wb.sock'
            bus_path.write_text(f'''<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig><type>session</type><listen>unix:path={escape(str(socket_path))}</listen><auth>EXTERNAL</auth><policy context="default"><allow user="{os.getuid()}"/><allow own="*"/><allow send_destination="*"/><allow receive_type="method_call"/><allow receive_type="method_return"/><allow receive_type="error"/><allow receive_type="signal"/></policy></busconfig>''')
            bar_bus = subprocess.Popen(['dbus-daemon', '--nofork', '--nopidfile', '--print-address=1', f'--config-file={bus_path}'], stdout=subprocess.PIPE, stderr=bar_log, text=True)
            address = bar_bus.stdout.readline().strip()
            assert address, 'Owned Waybar bus did not start'
            bar_env.update(DBUS_SESSION_BUS_ADDRESS=address, AT_SPI_BUS_ADDRESS=a11y, GIO_USE_VFS='local')
            bar = subprocess.Popen(['waybar', '--config', str(config_path), '--style', str(css_path)], env=bar_env, stdout=bar_log, stderr=bar_log, start_new_session=True)
            owned = bar_text('01:00')
            check('a separately launched real Waybar renders the Rust timer', True)
            # Wayland AT-SPI global coordinates are not trustworthy. Capture only if
            # the compositor explicitly identifies our own opaque layer by PID.
            def own_layer(value):
                if isinstance(value, dict):
                    if value.get('pid') == bar.pid and all(key in value for key in ['x', 'y', 'w', 'h']): return value
                    for item in value.values():
                        found = own_layer(item)
                        if found: return found
                elif isinstance(value, list):
                    for item in value:
                        found = own_layer(item)
                        if found: return found
                return None
            layer = own_layer(json.loads(subprocess.check_output(['hyprctl', '-j', 'layers'], text=True)))
            if layer and layer['w'] > 4 and layer['h'] > 4:
                subprocess.run(['grim', '-g', f"{layer['x'] + 2},{layer['y'] + 2} {layer['w'] - 4}x{layer['h'] - 4}", str(args.data / 'owned-waybar-label.png')], check=True)
        click('▶ Начать работу')
        running = state(); session_id = running['timer']['session']['id']; deadline = running['timer']['deadlineMs']
        frames.until(lambda f: f['class'] == 'focus')
        click('Свернуть в панель ↗')
        until = time.monotonic() + 3
        while time.monotonic() < until and not hidden(): time.sleep(.05)
        check('panel mode hides both owned windows, not just the sprite', hidden())
        first = frames.until(lambda f: f['class'] == 'focus' and f['text'] != '01:00')
        if bar is not None: bar_text(first['text'])
        stamp = (args.data / 'state.json').stat().st_mtime_ns
        frames.until(lambda f: f['class'] == 'focus' and f['text'] != first['text'])
        check('hidden windows do not stop the Rust stream or write checkpoints each second', hidden() and state()['timer']['deadlineMs'] == deadline and (args.data / 'state.json').stat().st_mtime_ns == stamp)
        show()
        check('panel click IPC reopens the same app without changing the session', state()['timer']['session']['id'] == session_id and state()['timer']['deadlineMs'] == deadline and find(app, lambda o: o.get_name() == 'Ⅱ Пауза') is not None)
        click('Ⅱ Пауза')
        paused = frames.until(lambda f: f['class'] == 'paused')
        paused_session = state()['timer']['session']
        check('pause has a frozen panel frame without idle polling', frames.next(2) is None and state()['timer']['session'] == paused_session and paused['text'].startswith('Ⅱ '))
        click('▶ Продолжить')
        frames.until(lambda f: f['class'] == 'focus')
        click('Свернуть в панель ↗')
        if args.full:
            time.sleep(61)
            frame = frames.until(lambda f: f['class'] == 'decision')
            check('a hidden real one-minute session reaches decision without adding waiting time', state()['timer']['status'] == 'AwaitingDecision' and sum(s['endMs'] - s['startMs'] for s in state()['timer']['session']['segments']) == 60000)
            check('Waybar shows completion but never the extension question or dialogue', frame['text'] == 'Готово' and 'пять минут' not in frame['tooltip'] and 'сэмпай' not in frame['tooltip'])
            if bar is not None:
                bar_text('Готово')
                check('the owned real Waybar follows completion with both WebViews hidden', hidden())
            show()
            until = time.monotonic() + 3
            while time.monotonic() < until and 'Ещё пять минут или заслуженный отдых' not in all_text(app): time.sleep(.05)
            check('the completion question is restored only after opening the large window', 'Ещё пять минут или заслуженный отдых' in all_text(app))
            click('Завершить и отдохнуть')
            rest = frames.until(lambda f: f['class'] == 'break')
            check('rest starts immediately and the panel reports the Rust rest countdown', state()['timer']['status'] == 'Break' and rest['text'].startswith('Отдых '))
            click('Свернуть в панель ↗')
            if bar is not None: bar_text('Отдых')
            frames.until(lambda f: f['class'] == 'break' and f['text'] != rest['text'])
            check('rest keeps counting with both owned windows hidden', hidden() and state()['timer']['status'] == 'Break')
            show()
            click('Закончить отдых »')
        else:
            show()
        click('↺ Сбросить')
        preserved = (state()['sessions'], state()['breaks'], state()['timer'])
        set_mode(False)
        frames.until(lambda f: f['class'] == 'disabled' and f['text'] == '')
        click('Помощник ↗')
        check('switching back restores the original companion without resetting accounting', find(app, lambda o: o.get_name() == 'Мику: клик — открыть, перетаскивание — переместить') is not None and (state()['sessions'], state()['breaks'], state()['timer']) == preserved)
        click('Мику: клик — открыть, перетаскивание — переместить')
        (args.data / 'panel-frames.json').write_text(json.dumps(frames.seen, ensure_ascii=False, indent=2))
    finally:
        if bar is not None:
            if bar.poll() is None:
                os.killpg(bar.pid, signal.SIGTERM)
                bar.wait(timeout=8)
            (args.data / 'private-waybar-exit.json').write_text(json.dumps({'pid': bar.pid, 'busPid': bar_bus.pid if bar_bus else None, 'exitCode': bar.returncode}))
        if bar_bus is not None and bar_bus.poll() is None:
            bar_bus.terminate()
            bar_bus.wait(timeout=8)
        if bar_log is not None: bar_log.close()
        if frames is not None: frames.stop()
    click('Сохранить и выйти')
