"""Read-only theme-source validation and owned Waybar restart smoke.

Never runs switch-theme.sh or pkill: only private HOME files, test PIDs and bus.
"""
import argparse
import ast
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import time
from xml.sax.saxutils import escape

from panel_smoke import Frames
from smoke_fixture import seed_silent_data


def jsonc(text):
    tokens = r'"(?:\\.|[^"\\])*"|//[^\n]*|/\*[\s\S]*?\*/'
    text = re.sub(tokens, lambda m: m[0] if m[0].startswith('"') else '', text)
    text = re.sub(r'"(?:\\.|[^"\\])*"|,\s*(?=[}\]])', lambda m: m[0] if m[0].startswith('"') else '', text)
    return json.loads(text)


def stop(process, group=False):
    if process is not None and process.poll() is None:
        if group: os.killpg(process.pid, signal.SIGTERM)
        else: process.terminate()
        try: process.wait(timeout=8)
        except subprocess.TimeoutExpired:
            if group: os.killpg(process.pid, signal.SIGKILL)
            else: process.kill()
            process.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--themes', type=Path, default=Path.home() / '.config/hypr/themes')
    parser.add_argument('--rounds', type=int, default=2)
    args = parser.parse_args()
    assert 1 <= args.rounds <= 3
    assert jsonc('{/*c*/"url":"http://local//x", "x":[1,], //c\n}') == {'url': 'http://local//x', 'x': [1]}
    root = Path(__file__).resolve().parent.parent
    themes = args.themes.resolve()
    names = ['red', 'angel', 'modern', 'aurora', 'tty']
    sources = {name: themes / name / 'waybar.jsonc' if (themes / name / 'waybar.jsonc').exists() else themes / '_base/waybar.jsonc' for name in names}
    common = Path.home() / '.config/waybar/miku-focus.jsonc'
    shared = jsonc(common.read_text())
    assert set(shared) == {'custom/miku-focus'}
    assert '--waybar' in shared['custom/miku-focus']['exec']
    assert '--show-main' in shared['custom/miku-focus']['on-click']
    assert 'interval' not in shared['custom/miku-focus']
    for name, source in sources.items():
        config = jsonc(source.read_text())
        assert str(common) in config['include'], name
        assert sum(config.get(key, []).count('custom/miku-focus') for key in ['modules-left', 'modules-center', 'modules-right']) == 1, name
        assert 'custom/miku-focus' not in config, 'Definition must stay shared'
        assert str(common.with_suffix('.css')) in (themes / name / 'style.css').read_text(), name

    import gi
    gi.require_version('Atspi', '2.0')
    gi.require_version('Gtk', '3.0')
    from gi.repository import Atspi, GLib, Gtk, Gdk
    # Parse the actual GTK styles and verify inherited foreground without mapping a window.
    colors = {}
    for name in names:
        provider = Gtk.CssProvider()
        errors = []
        provider.connect('parsing-error', lambda _, section, error: errors.append(str(error)))
        provider.load_from_path(str(themes / name / 'style.css'))
        assert not errors, (name, errors)
        screen = Gdk.Screen.get_default()
        Gtk.StyleContext.add_provider_for_screen(screen, provider, Gtk.STYLE_PROVIDER_PRIORITY_USER)
        window = Gtk.Window(); window.set_name('waybar')
        label = Gtk.Label(label='25:00'); label.set_name('custom-miku-focus'); window.add(label)
        colors[name] = label.get_style_context().get_color(Gtk.StateFlags.NORMAL).to_string()
        window.destroy()
        Gtk.StyleContext.remove_provider_for_screen(screen, provider)
    assert len(set(colors.values())) == 5, colors

    base = root / '.local-test' / f'waybar-themes-{time.time_ns()}'
    base.mkdir(mode=0o700)
    print('Owned theme test:', base, flush=True)
    sandbox = base / 'home'
    private_themes = sandbox / '.config/hypr/themes'
    private_bar = sandbox / '.config/waybar'
    private_bar.mkdir(parents=True)
    (private_themes / '_base').mkdir(parents=True)
    for name in ['_base', *names]:
        (private_themes / name).mkdir(exist_ok=True)
        for file in ['waybar.jsonc', 'style.css']:
            source = themes / name / file
            if source.exists(): shutil.copy2(source, private_themes / name / file)
    for file in ['kitty-appearance.conf', 'hyprlock-layout.conf']:
        (private_themes / '_base' / file).write_text('# Private smoke placeholder\n')
    for directory in ['kitty', 'hypr/scripts']:
        (sandbox / '.config' / directory).mkdir(parents=True, exist_ok=True)
    scripts = themes.parent / 'scripts'
    shutil.copy2(scripts / 'apply-theme.sh', sandbox / '.config/hypr/scripts/apply-theme.sh')
    (private_themes / 'current').write_text('tty\n')
    apply_env = dict(os.environ, HOME=str(sandbox))
    # The real startup script copies saved tty sources in an isolated HOME. No wallpaper,
    # borders or appearance files are copied, so it cannot dispatch compositor changes.
    subprocess.run(['bash', str(scripts / 'startup-theme.sh')], env=apply_env, check=True, timeout=10)
    assert jsonc((private_bar / 'config.jsonc').read_text())['include'] == [str(common)]

    data = base / 'data/dev.mikufocus.desktop'
    seed_silent_data(data)
    path = data / 'state.json'
    state = json.loads(path.read_text())
    state['settings'].update(backgroundMode='Panel', quietMode=True, talkativeness='Disabled', textSpeed=0)
    path.write_text(json.dumps(state))
    env = dict(os.environ, XDG_DATA_HOME=str(base / 'data'), XDG_CONFIG_HOME=str(base / 'config'), XDG_CACHE_HOME=str(base / 'cache'), GDK_BACKEND='wayland', GSETTINGS_BACKEND='memory', PULSE_PROP='application.name=miku-theme-smoke module-stream-restore.id=miku-theme-smoke')
    binary = root / 'src-tauri/target/release/miku-focus'
    app = bar = bus = frames = None
    checks = []
    def check(name, condition=True):
        assert condition, name
        checks.append(name); print('PASS', name, flush=True)
    def pump():
        ctx = GLib.MainContext.default()
        while ctx.pending(): ctx.iteration(False)
    def owned(pid):
        pump(); desktop = Atspi.get_desktop(0)
        return next((desktop.get_child_at_index(i) for i in range(desktop.get_child_count()) if desktop.get_child_at_index(i).get_process_id() == pid), None)
    def find(obj, predicate):
        if obj is None: return None
        try:
            obj.clear_cache()
            if predicate(obj): return obj
            for i in range(obj.get_child_count()):
                result = find(obj.get_child_at_index(i), predicate)
                if result is not None: return result
        except GLib.Error: pass
        return None
    def click(name):
        until = time.monotonic() + 6
        while time.monotonic() < until:
            control = find(owned(app.pid), lambda o: o.get_name() == name and o.get_action_iface() is not None)
            if control is not None and control.get_action_iface().do_action(0): return
            time.sleep(.05)
        raise AssertionError(f'Owned control missing: {name}')
    def timer_text():
        label = find(owned(bar.pid), lambda o: o.get_text_iface() is not None and re.fullmatch(r'\d{2}:\d{2}', Atspi.Text.get_text(o, 0, -1)) is not None)
        return Atspi.Text.get_text(label, 0, -1) if label else None
    def cli_pids():
        result = []
        for proc in Path('/proc').iterdir():
            if not proc.name.isdecimal(): continue
            try:
                if '--waybar'.encode() not in (proc / 'cmdline').read_bytes().split(b'\0'): continue
                if (proc / 'exe').resolve() != binary: continue
                process_env = dict(v.split(b'=', 1) for v in (proc / 'environ').read_bytes().split(b'\0') if b'=' in v)
                if process_env.get(b'XDG_DATA_HOME') == str(base / 'data').encode(): result.append(int(proc.name))
            except OSError: pass
        return result
    def until(predicate, timeout=8):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            if predicate(): return
            time.sleep(.05)
        raise AssertionError('Owned theme test timed out')

    try:
        log = (base / 'app.log').open('w')
        app = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=log)
        until(lambda: owned(app.pid) is not None)
        click('▶ Начать работу')
        until(lambda: json.loads(path.read_text())['timer']['status'] == 'Focus')
        before = json.loads(path.read_text())['timer']
        click('Свернуть в панель ↗')
        frames = Frames(binary, env)
        frames.until(lambda f: f['class'] == 'focus')
        a11y = env.get('AT_SPI_BUS_ADDRESS') or ast.literal_eval(subprocess.check_output(['gdbus', 'call', '--session', '--dest', 'org.a11y.Bus', '--object-path', '/org/a11y/bus', '--method', 'org.a11y.Bus.GetAddress'], text=True))[0]
        bus_config = base / 'bus.conf'
        bus_config.write_text(f'<busconfig><type>session</type><listen>unix:path={escape(str(base / "bus.sock"))}</listen><auth>EXTERNAL</auth><policy context="default"><allow user="{os.getuid()}"/><allow own="*"/><allow send_destination="*"/><allow receive_type="method_call"/><allow receive_type="method_return"/><allow receive_type="error"/><allow receive_type="signal"/></policy></busconfig>')
        bus_log = (base / 'bus.log').open('w')
        bus = subprocess.Popen(['dbus-daemon', '--nofork', '--nopidfile', '--print-address=1', f'--config-file={bus_config}'], stdout=subprocess.PIPE, stderr=bus_log, text=True)
        address = bus.stdout.readline().strip(); assert address
        bar_env = dict(env, DBUS_SESSION_BUS_ADDRESS=address, AT_SPI_BUS_ADDRESS=a11y, GIO_USE_VFS='local')
        records = []
        for turn in range(args.rounds):
            for name in names:
                subprocess.run(['bash', str(scripts / 'apply-theme.sh'), name], env=apply_env, check=True, capture_output=True, timeout=10)
                config = jsonc((private_bar / 'config.jsonc').read_text())
                # Exercise the exact include and CSS, but no unrelated widgets/tray/commands,
                # exclusive zones or live desktop theme changes in a smoke test.
                config.update(name='miku-theme-smoke', layer='overlay', exclusive=False, position='bottom', width=240, **{'modules-left': [], 'modules-center': ['custom/miku-focus'], 'modules-right': [], 'margin-bottom': 42})
                config_path = base / 'test-waybar.jsonc'; config_path.write_text(json.dumps(config))
                bar_log = (base / f'{turn}-{name}.log').open('w')
                bar = subprocess.Popen(['waybar', '--config', str(config_path), '--style', str(private_bar / 'style.css')], env=bar_env, stdout=bar_log, stderr=bar_log, start_new_session=True)
                until(lambda: timer_text() is not None)
                first = timer_text()
                until(lambda: timer_text() is not None and timer_text() != first)
                until(lambda: len(cli_pids()) == 2)
                current = json.loads(path.read_text())['timer']
                check(f'{turn + 1}/{name}: native themed timer updates after a fresh Waybar PID', current['session']['id'] == before['session']['id'] and current['deadlineMs'] == before['deadlineMs'] and app.poll() is None)
                records.append({'round': turn + 1, 'theme': name, 'waybarPid': bar.pid, 'clients': cli_pids(), 'first': first, 'next': timer_text(), 'foreground': colors[name]})
                stop(bar, group=True); until(lambda: len(cli_pids()) == 1)
                check(f'{turn + 1}/{name}: stopping Waybar releases its client without duplicates')
                bar_log.close(); bar = None
        subprocess.run([str(binary), '--show-main'], env=env, check=True, timeout=8)
        until(lambda: find(owned(app.pid), lambda o: o.get_name() == 'Ⅱ Пауза') is not None)
        check('the same private app reopens after all panel restarts')
        check('startup-theme.sh restores included tty module in isolated HOME')
        check('five GTK CSS palettes parse cleanly and inherit five distinct theme colors')
        (base / 'results.json').write_text(json.dumps({'passed': True, 'checks': checks, 'count': len(checks), 'rounds': args.rounds, 'records': records, 'colors': colors, 'realLoginTested': False, 'globalThemeSwitchRun': False}, ensure_ascii=False, indent=2))
    finally:
        stop(bar, group=True)
        if frames: frames.stop()
        stop(app); stop(bus)
        until(lambda: not cli_pids())
        (base / 'cleanup.json').write_text(json.dumps({'appStopped': app is None or app.poll() is not None, 'busStopped': bus is None or bus.poll() is not None, 'clients': cli_pids()}))


if __name__ == '__main__':
    main()
