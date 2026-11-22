"""Real WebKitGTK accessibility/IPC smoke test, scoped to an isolated test instance.

Requires existing Arch python-gobject/Atspi; no browser or WebDriver download.
Launch with XDG_DATA_HOME=$PWD/.local-test/native-data first. Never run on personal data.
"""
import argparse
import json
from pathlib import Path
import subprocess
import time
import gi
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi, GLib
from hypr_smoke import dispatch as hypr_dispatch

p = argparse.ArgumentParser()
p.add_argument('--pid', type=int, required=True)
p.add_argument('--data', type=Path, required=True)
p.add_argument('--full', action='store_true', help='Wait for a real one-minute focus deadline')
p.add_argument('--x11', action='store_true', help='Private Xvfb/DBus session; not a Wayland verification')
p.add_argument('--obsidian-only', action='store_true', help='Check only the native export-folder connection')
p.add_argument('--panel-only', action='store_true', help='Check the owned Rust stream and panel window mode')
p.add_argument('--quit-only', choices=['main', 'companion'], help='Check farewell and bounded shutdown only')
p.add_argument('--quiet-exit', action='store_true', help='Check the farewell with Quiet Mode enabled')
p.add_argument('--pointer-helper', type=Path, help='Existing Wayland virtual-pointer test helper, only for this owned window')
args = p.parse_args()
if '.local-test' not in args.data.resolve().parts:
    raise SystemExit('Refusing to run outside .local-test: do not mutate personal history')
if args.x11 and args.pointer_helper:
    raise SystemExit('The Wayland pointer helper must not be used for an X11 test')
root = Atspi.get_desktop(0)
app = next(root.get_child_at_index(i) for i in range(root.get_child_count())
           if root.get_child_at_index(i).get_process_id() == args.pid)
checks = []
approved_bindings = json.loads((Path(__file__).resolve().parent.parent / 'src/lib/sound-catalog.json').read_text())['defaults']

def find(obj, predicate):
    try:
        if predicate(obj): return obj
    except GLib.Error: pass
    try: count = obj.get_child_count()
    except GLib.Error: return None
    for i in range(count):
        try: child = obj.get_child_at_index(i)
        except GLib.Error: continue
        if child is None: continue
        match = find(child, predicate)
        if match: return match
    return None

def click(name):
    # A resize/remount may invalidate WebKit's accessibility subtree for a frame.
    until = time.monotonic() + 3
    while time.monotonic() < until:
        obj = find(app, lambda o: o.get_name() == name and o.get_action_iface() is not None)
        try:
            if obj is not None and obj.get_action_iface().do_action(0): break
        except GLib.Error: pass
        app.clear_cache()
        time.sleep(.05)
    else: raise AssertionError(f'Control not found or not actionable: {name}')
    time.sleep(0.25)

increase = 'Увеличить время работы на минуту'
decrease = 'Уменьшить время работы на минуту'

def burst(name, count):
    obj = find(app, lambda o: o.get_name() == name and o.get_action_iface() is not None)
    assert obj is not None, name
    for _ in range(count): assert obj.get_action_iface().do_action(0), name

def disabled(name):
    obj = find(app, lambda o: o.get_name() == name and o.get_action_iface() is not None)
    return obj is not None and not obj.get_state_set().contains(Atspi.StateType.ENABLED)

def all_text(obj):
    result = Atspi.Text.get_text(obj, 0, -1) if obj.get_text_iface() else ''
    return result + ''.join(all_text(obj.get_child_at_index(i)) for i in range(obj.get_child_count()))

def settings_duration(minutes):
    click('Настройки'); click('Таймер')
    entry('Работа, мин', minutes)
    assert persisted(lambda s: s['settings']['focusMinutes'] == minutes)
    click('♡ Комната фокуса')

def show_focus():
    if find(app, lambda o: o.get_name() == 'Мику: клик — открыть, перетаскивание — переместить'):
        click('Мику: клик — открыть, перетаскивание — переместить')
    click('♡ Комната фокуса')

def client():
    if args.x11:
        window = subprocess.check_output(['xdotool', 'search', '--all', '--onlyvisible', '--pid', str(args.pid), '--name', '^Miku Focus'], text=True).splitlines()[0]
        geometry = dict(line.split('=', 1) for line in subprocess.check_output(['xdotool', 'getwindowgeometry', '--shell', window], text=True).splitlines())
        return {'address': window, 'at': [int(geometry['X']), int(geometry['Y'])], 'size': [int(geometry['WIDTH']), int(geometry['HEIGHT'])]}
    return next(c for c in json.loads(subprocess.check_output(['hyprctl', '-j', 'clients'])) if c['pid'] == args.pid)

def entry(prefix, value):
    obj = find(app, lambda o: o.get_role_name() == 'entry' and o.get_name().startswith(prefix))
    assert obj, prefix
    address = client()['address']
    if args.x11:
        subprocess.run(['xdotool', 'windowfocus', '--sync', address], check=True)
        assert obj.get_component_iface().grab_focus()
        subprocess.run(['xdotool', 'key', '--clearmodifiers', 'ctrl+a'], check=True)
        subprocess.run(['xdotool', 'type', '--clearmodifiers', '--delay', '30', str(value)], check=True)
        time.sleep(.15)
        return
    hypr_dispatch(args.pid, address, 'focus')
    assert obj.get_component_iface().grab_focus()
    # Physical A key: independent of the user's active EN/RU keyboard layout.
    for mods, key in [('CTRL', 'code:38'), *[('', char) for char in str(value)]]:
        hypr_dispatch(args.pid, address, 'shortcut', mods, key)
        time.sleep(0.06)

def entry_text(prefix):
    obj = find(app, lambda o: o.get_role_name() == 'entry' and o.get_name().startswith(prefix))
    assert obj, prefix
    return Atspi.Text.get_text(obj, 0, -1)

def keys(obj, *names):
    address = client()['address']
    if args.x11:
        subprocess.run(['xdotool', 'windowfocus', '--sync', address], check=True)
    else:
        hypr_dispatch(args.pid, address, 'focus')
    assert obj.get_component_iface().grab_focus()
    for key in names:
        if args.x11:
            subprocess.run(['xdotool', 'key', '--clearmodifiers', key], check=True)
        else:
            hypr_dispatch(args.pid, address, 'shortcut', '', key)
        time.sleep(.08)
    time.sleep(.2)

def upload_sound(control, filename):
    assert args.x11, 'File-picker typing is tested only in the private X11 session'
    click(control)
    # Only this owned application's Gtk chooser; no global search by window class/title.
    chooser = find(app, lambda o: o.get_role_name() == 'file chooser')
    assert chooser is not None, 'Owned Gtk file chooser did not open'
    subprocess.run(['xdotool', 'key', '--clearmodifiers', 'ctrl+l'], check=True)
    subprocess.run(['xdotool', 'type', '--clearmodifiers', '--delay', '2', str(filename.resolve())], check=True)
    subprocess.run(['xdotool', 'key', '--clearmodifiers', 'Return'], check=True)
    time.sleep(.4)
    confirm = find(chooser, lambda o: o.get_name().lstrip('_') in ['Открыть', 'Open'] and o.get_action_iface() is not None)
    if confirm is not None: assert confirm.get_action_iface().do_action(0)
    time.sleep(.6)

def state(): return json.loads((args.data / 'state.json').read_text())
def check(name, condition):
    if not condition:
        (args.data.parent / 'smoke-failure.txt').write_text(all_text(app))
        screenshot('smoke-failure.png')
    assert condition, name
    checks.append(name)
    print('PASS', name, flush=True)

def persisted(predicate):
    until = time.monotonic() + 5
    while time.monotonic() < until:
        if predicate(state()): return True
        time.sleep(.05)
    return False

export_vault = args.data.parent.parent / 'vault'
export_path = export_vault / 'Miku Focus/Данные/linux.json'

def exported(predicate):
    until = time.monotonic() + 5
    while time.monotonic() < until:
        try:
            if predicate(json.loads(export_path.read_text())): return True
        except (FileNotFoundError, json.JSONDecodeError): pass
        time.sleep(.05)
    return False

def select_export_folder(cancel=False):
    assert args.x11 or cancel, 'Native folder-picker text input is checked only in private Xvfb'
    click('Изменить папку хранилища' if (args.data / 'obsidian.json').exists() and json.loads((args.data / 'obsidian.json').read_text())['folder'] else 'Выбрать папку хранилища')
    app.clear_cache()
    chooser = find(app, lambda o: o.get_role_name() == 'file chooser' and o.get_state_set().contains(Atspi.StateType.SHOWING))
    assert chooser is not None, 'Owned native folder chooser did not open'
    if cancel:
        button = find(chooser, lambda o: o.get_name().lstrip('_') in ['Отмена', 'Cancel'] and o.get_action_iface() is not None)
        assert button is not None and button.get_action_iface().do_action(0)
    else:
        window = subprocess.check_output(['xdotool', 'search', '--all', '--onlyvisible', '--pid', str(args.pid), '--name', '^Выберите папку хранилища для выгрузки$'], text=True).splitlines()[0]
        # Private Xvfb has no window manager to constrain/centre a large GTK chooser.
        subprocess.run(['xdotool', 'windowsize', window, '1000', '700'], check=True)
        subprocess.run(['xdotool', 'windowmove', window, '10', '10'], check=True)
        subprocess.run(['xdotool', 'windowfocus', '--sync', window], check=True)
        time.sleep(.25)
        home = find(chooser, lambda o: o.get_name() == 'Домашняя папка' and o.get_state_set().contains(Atspi.StateType.SHOWING))
        if home is not None:
            rect = home.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
            subprocess.run(['xdotool', 'mousemove', str(rect.x + rect.width // 2), str(rect.y + rect.height // 2), 'click', '1'], check=True)
            time.sleep(.25)
        subprocess.run(['xdotool', 'key', '--clearmodifiers', 'ctrl+l'], check=True)
        time.sleep(.15)
        subprocess.run(['xdotool', 'key', '--clearmodifiers', 'ctrl+a'], check=True)
        subprocess.run(['xdotool', 'type', '--clearmodifiers', '--delay', '2', str(export_vault.resolve())], check=True)
        # Confirm the typed path once; a second acceptance could select a generated child.
        time.sleep(.15)
        button = find(chooser, lambda o: o.get_name().lstrip('_') in ['Выбрать', 'Выбрать папку', 'Открыть', 'Select', 'Open'] and o.get_state_set().contains(Atspi.StateType.SHOWING) and o.get_state_set().contains(Atspi.StateType.ENABLED))
        assert button is not None, 'Native accept button must be enabled'
        rect = button.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
        assert rect.width > 0 and rect.height > 0, 'Native accept button must be visible'
        subprocess.run(['xdotool', 'mousemove', str(rect.x + rect.width // 2), str(rect.y + rect.height // 2), 'click', '1'], check=True)
        time.sleep(.4)
        def describe(obj):
            try:
                return {'role': obj.get_role_name(), 'name': obj.get_name(), 'showing': obj.get_state_set().contains(Atspi.StateType.SHOWING), 'text': Atspi.Text.get_text(obj, 0, -1) if obj.get_text_iface() else '', 'children': [describe(obj.get_child_at_index(i)) for i in range(obj.get_child_count())]}
            except GLib.Error: return None
        (args.data / 'folder-picker.json').write_text(json.dumps(describe(chooser), ensure_ascii=False))
        if not export_path.exists():
            gi.require_version('Gdk', '3.0')
            from gi.repository import Gdk
            geometry = dict(line.split('=', 1) for line in subprocess.check_output(['xdotool', 'getwindowgeometry', '--shell', window], text=True).splitlines())
            image = Gdk.pixbuf_get_from_window(Gdk.get_default_root_window(), *[int(geometry[key]) for key in ['X', 'Y', 'WIDTH', 'HEIGHT']])
            if image is not None: image.savev(str(args.data / 'folder-picker-window.png'), 'png', [], [])
    time.sleep(.6)

def geometry_settled(predicate):
    # Hyprland IPC exposes interpolated geometry while the user's animations run.
    until = time.monotonic() + 5
    while time.monotonic() < until:
        c = client()
        if predicate(c): return c
        time.sleep(.15)
    raise AssertionError(f'Compositor geometry did not settle: at={c["at"]}, size={c["size"]}')

def screenshot(name):
    c = client()
    if args.x11:
        gi.require_version('Gdk', '3.0')
        from gi.repository import Gdk
        image = Gdk.pixbuf_get_from_window(Gdk.get_default_root_window(), *c['at'], *c['size'])
        assert image is not None, 'Could not capture the private Xvfb test window'
        if name == 'exit-black.png':
            # WebKit may schedule the first animation frame later after remounting
            # focus from Settings. Wait for the actual black frame, not a guessed instant.
            deadline = time.monotonic() + 1.8
            while time.monotonic() < deadline:
                pixels = image.get_pixels(); stride = image.get_rowstride(); channels = image.get_n_channels()
                points = [(image.get_width() // 2, image.get_height() // 2), (image.get_width() - 8, image.get_height() // 2), (8, 8)]
                if all(max(pixels[y * stride + x * channels:y * stride + x * channels + 3]) <= 3 for x, y in points): break
                time.sleep(.05)
                try:
                    c = client(); Gdk.Display.get_default().sync()
                    next_image = Gdk.pixbuf_get_from_window(Gdk.get_default_root_window(), *c['at'], *c['size'])
                    if next_image is None: break
                    image = next_image
                except (subprocess.CalledProcessError, IndexError, GLib.Error): break
        image.savev(str(args.data.parent / name), 'png', [], [])
        if name == 'exit-black.png':
            pixels = image.get_pixels(); stride = image.get_rowstride(); channels = image.get_n_channels()
            points = [(image.get_width() // 2, image.get_height() // 2), (image.get_width() - 8, image.get_height() // 2), (8, 8)]
            check('shutdown reaches opaque black including the old scrollbar area', all(max(pixels[y * stride + x * channels:y * stride + x * channels + 3]) <= 3 for x, y in points))
        return
    hypr_dispatch(args.pid, c['address'], 'focus')
    time.sleep(.3)
    c = client(); x, y = c['at']; w, h = c['size']
    subprocess.run(['grim', '-g', f'{x},{y} {w}x{h}', str(args.data.parent / name)], check=True)

def finish_exit_checks():
    if args.quiet_exit:
        assert persisted(lambda s: s['settings']['quietMode'])
    if args.x11:
        # Xvfb has no WM; emulate the focus a real click gives the foreground WebView.
        subprocess.run(['xdotool', 'windowfocus', '--sync', client()['address']], check=True)
    started = time.monotonic()
    click('Сохранить и выйти')
    check('quit displays a black-fade presentation instead of exiting immediately', find(app, lambda o: o.get_name() == 'Завершение работы') is not None and Path(f'/proc/{args.pid}').exists())
    farewell = 'Я буду ждать нашей следующей встречи ♡'
    if args.quit_only == 'companion':
        dialogue = find(app, lambda o: o.get_text_iface() is not None and Atspi.Text.get_text(o, 0, -1) == farewell)
    else:
        dialogue = find(app, lambda o: o.get_name() == 'Русские субтитры')
    check('quit shows the farewell in the existing dialogue surface without requiring a click', dialogue is not None and farewell in all_text(dialogue))
    check('quit has no separate farewell card', find(app, lambda o: o.get_name() == 'Прощание Мику') is None)
    screenshot('exit-farewell.png')
    if not args.quiet_exit:
        # Farewell, 1.5s fade and 2.5s close SFX now start together.
        # Inspect black while the longer SFX is still keeping the process alive.
        time.sleep(max(0, started + 1.8 - time.monotonic()))
        # Transparent companion surfaces may present animation frames later than
        # the opaque main window. Inspect their fade, not black at a fixed instant.
        screenshot('exit-fade.png' if args.quit_only == 'companion' else 'exit-black.png')

if args.quit_only:
    before = (state()['sessions'], state()['breaks'], state()['timer'])
    if args.quiet_exit:
        click('Переключить тихий режим')
        assert persisted(lambda s: s['settings']['quietMode'])
    if args.quit_only == 'companion':
        click('Помощник ↗')
        character = find(app, lambda o: o.get_name() == 'Мику: клик — открыть, перетаскивание — переместить')
        assert character is not None
        if args.x11:
            c = client(); x, y = c['at']; w, h = c['size']
            subprocess.run(['xdotool', 'mousemove', str(x + w // 2), str(y + h // 2), 'click', '3'], check=True)
            time.sleep(.2)
        else:
            keys(character, 'Menu')
    else:
        click('Настройки')  # Exit must restore the ordinary dialogue even from another tab.
    finish_exit_checks()
    check('farewell leaves timer and recorded history unchanged', (state()['sessions'], state()['breaks'], state()['timer']) == before)
    (args.data.parent / 'smoke-results.json').write_text(json.dumps({'backend': 'X11/Xvfb' if args.x11 else 'Wayland/Hyprland', 'checks': checks, 'count': len(checks)}))
    raise SystemExit(0)

if args.panel_only:
    from panel_smoke import run
    run(args, app, click=click, find=find, keys=keys, state=state, persisted=persisted, check=check, client=client, all_text=all_text, settings_duration=settings_duration)
    (args.data.parent / 'smoke-results.json').write_text(json.dumps({'backend': 'X11/Xvfb' if args.x11 else 'Wayland/Hyprland', 'checks': checks, 'count': len(checks)}))
    raise SystemExit(0)

if args.obsidian_only:
    click('Настройки'); click('Система')
    export_vault.mkdir(exist_ok=True)
    select_export_folder()
    check('native export folder selection connects and writes JSON', exported(lambda d: d['format'] == 'miku-focus-statistics'))
    click('Отключить автовыгрузку')
    select_export_folder()
    check('native reconnect selects the vault itself, not a generated child directory', json.loads((args.data / 'obsidian.json').read_text())['folder'] == str(export_vault.resolve()))
    click('Сохранить и выйти')
    (args.data.parent / 'smoke-results.json').write_text(json.dumps({'backend': 'X11/Xvfb', 'checks': checks, 'count': len(checks)}))
    raise SystemExit(0)

time.sleep(1.5)
check('launch dialogue is shown', 'С возвращением, сэмпай' in all_text(app))
time.sleep(5)
check('launch dialogue does not advance automatically', 'С возвращением, сэмпай' in all_text(app))
click('Продолжить диалог')
check('clicking the dialogue panel continues the launch message', 'С возвращением, сэмпай' not in all_text(app))

click('Настройки')
click('Таймер')
entry('Работа, мин', 10)
entry('Короткий отдых, мин', 1)
entry('Длинный отдых, мин', 1)
entry('Сессий перед длинным отдыхом', 2)
check('break settings autosave through real DOM input and IPC', persisted(lambda s: s['settings']['shortBreakMinutes'] == 1 and s['settings']['longBreakMinutes'] == 1 and s['settings']['sessionsBeforeLong'] == 2))
check('settings have no save button', find(app, lambda o: o.get_name() == 'Сохранить ♡') is None)
work_input = find(app, lambda o: o.get_role_name() == 'entry' and o.get_name().startswith('Работа, мин'))
keys(work_input, 'Up')
check('numeric setting up arrow autosaves an increment', persisted(lambda s: s['settings']['focusMinutes'] == 11))
keys(work_input, 'Down')
check('numeric setting down arrow autosaves a decrement', persisted(lambda s: s['settings']['focusMinutes'] == 10))
if args.x11:
    rect = work_input.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
    # The input has 9px right padding; the native stepper is inside that padding, not at its border.
    for fraction, expected in [(.35, 11), (.65, 10)]:
        subprocess.run(['xdotool', 'mousemove', str(rect.x + rect.width - 18), str(rect.y + int(rect.height * fraction)), 'click', '1'], check=True)
        check('native mouse spinner changes the setting to ' + str(expected), persisted(lambda s: s['settings']['focusMinutes'] == expected))
click('Мику и голос')
check('legacy-compatible default text speed is available', entry_text('Скорость появления символов, симв/с') == '60')
click('Мику и голос')
check('repeated inner settings tab click keeps the current view and value', entry_text('Скорость появления символов, симв/с') == '60')
entry('Скорость появления символов, симв/с', 0)
check('instant text speed autosaves', persisted(lambda s: s['settings']['textSpeed'] == 0))
entry('Скорость появления символов, симв/с', 120)
check('custom text speed autosaves', persisted(lambda s: s['settings']['textSpeed'] == 120))
speed_input = find(app, lambda o: o.get_name().startswith('Скорость появления символов, симв/с') and o.get_role_name() == 'entry')
keys(speed_input, 'Up')
check('text speed arrow uses the shared numeric control', persisted(lambda s: s['settings']['textSpeed'] == 121))
keys(speed_input, 'Down')
assert persisted(lambda s: s['settings']['textSpeed'] == 120)
screenshot('settings.png')
click('Звуки')
check('legacy data gains the six approved bindings and all nine supplied effects', find(app, lambda o: o.get_name() == 'Файлы звуковых эффектов') is not None and state()['settings']['sfxBindings'] == approved_bindings and all(name in all_text(app) for name in ['menu-open.wav', 'menu-close.wav', 'choice-hover.wav', 'high.wav', 'low.wav', 'soft.wav', 'UI SFX_InGameMenu_Open.ogg', 'UI SFX_InGameMenu_Close.ogg', 'UI SFX_MENU_Hover.ogg']))
click('Эффекты (9)')
click('Эффекты (9)')
check('repeated sound tab clicks do not start or retain preview playback', disabled('■ Остановить'))
screenshot('sounds-effects.png')
click('Громкость и тихий режим')
keys(find(app, lambda o: o.get_name() == 'Громкость эффектов'), 'Home')
check('effect volume autosaves independently at zero for silent playback checks', persisted(lambda s: s['settings']['sfxVolume'] == 0 and s['settings']['volume'] == .7))
voice_volume = find(app, lambda o: o.get_name() == 'Громкость голоса')
keys(voice_volume, 'Left')
check('volume range accepts a left arrow step without changing effect volume', persisted(lambda s: abs(s['settings']['volume'] - .65) < .0001 and s['settings']['sfxVolume'] == 0))
keys(voice_volume, 'Right')
assert persisted(lambda s: abs(s['settings']['volume'] - .7) < .0001)
click('Громкость и тихий режим')
entry('Поиск звуков', 123)
check('sound search filters the table', 'Ничего не найдено.' in all_text(app))
search_entry = find(app, lambda o: o.get_name() == 'Поиск звуков')
keys(search_entry, 'Home', 'Delete', 'Delete', 'Delete')
check('clearing search restores effects', 'menu-open.wav' in all_text(app))
preview_stop = find(app, lambda o: o.get_name() == '■ Остановить')
keys(find(app, lambda o: o.get_name() == 'Прослушать UI SFX_InGameMenu_Open.ogg'), 'space')
check('long OGG preview starts at zero volume', preview_stop.get_state_set().contains(Atspi.StateType.ENABLED))
assert preview_stop.get_action_iface().do_action(0), 'Stop the owned preview before its natural end'
time.sleep(.25)
check('preview stop resets table playback state', disabled('■ Остановить'))
click('Прослушать UI SFX_InGameMenu_Open.ogg')
click('Переключить тихий режим')
check('Quiet Mode stops preview and disables audition', persisted(lambda s: s['settings']['quietMode']) and disabled('■ Остановить') and disabled('Прослушать high.wav'))
click('Переключить тихий режим')
assert persisted(lambda s: not s['settings']['quietMode'])
click(f'Назначения эффектов ({len(approved_bindings)})')
keys(find(app, lambda o: o.get_name() == 'Звук: Начало работы'), 'Home', 'Down', 'Return', 'Escape')
assert persisted(lambda s: s['settings']['sfxBindings'].get('start') == 'menu-open')
keys(find(app, lambda o: o.get_name() == 'Звук: Пауза работы'), 'Home', 'Down', 'Down', 'Return', 'Escape')
check('individual event assignments preserve approved button and exit bindings', persisted(lambda s: s['settings']['sfxBindings'] == {**approved_bindings, 'start': 'menu-open', 'pause': 'menu-close'}))
keys(find(app, lambda o: o.get_name() == 'Звук: Начало работы'), 'Home', 'Return', 'Escape')
keys(find(app, lambda o: o.get_name() == 'Звук: Пауза работы'), 'Home', 'Return', 'Escape')
check('extra timer assignments can be cleared without changing button bindings', persisted(lambda s: s['settings']['sfxBindings'] == approved_bindings))
click(f'Назначения эффектов ({len(approved_bindings)})')
voice_tab = find(app, lambda o: o.get_name().startswith('Голоса (') and o.get_action_iface() is not None)
click(voice_tab.get_name())
check('voice table shows Russian lines, IDs, missing files and disabled previews', 'С возвращением, сэмпай' in all_text(app) and 'welcome' in all_text(app) and 'Нет файла' in all_text(app) and disabled('Прослушать голос welcome'))
screenshot('sounds-voices.png')
if args.x11:
    original = Path(__file__).resolve().parent.parent / 'sfx/menu-open.wav'
    upload_sound('Загрузить голос welcome', original)
    check('voice row replacement imports binary audio under the selected ID', persisted(lambda s: 'welcome' in s['settings']['voices']))
    voice_copy = Path(state()['settings']['voices']['welcome'])
    check('voice import is byte-identical to the supplied file', voice_copy.read_bytes() == original.read_bytes())
    click('Отвязать голос welcome')
    check('clearing voice association keeps physical files', persisted(lambda s: not s['settings']['voices']) and voice_copy.exists() and original.exists())
click('Эффекты (9)')
if args.x11:
    upload_sound('Заменить эффект high.wav', original)
    check('bundled effect can be replaced with local audio', persisted(lambda s: 'high' in s['settings']['sfx']))
    effect_copy = Path(state()['settings']['sfx']['high'])
    click('Вернуть встроенный')
    check('resetting an effect restores bundled fallback without deleting its copy', persisted(lambda s: not s['settings']['sfx']) and effect_copy.read_bytes() == original.read_bytes())
    upload_sound('Добавить звуковые эффекты', original)
    check('adding an effect uses a new ID without overwriting bundled assets', persisted(lambda s: 'custom-menu-open' in s['settings']['sfx']))
    custom_copy = Path(state()['settings']['sfx']['custom-menu-open'])
    click('Убрать из списка')
    check('removing an imported effect keeps the source and app-data copy', persisted(lambda s: not s['settings']['sfx']) and custom_copy.exists() and original.exists())
click('Прослушать UI SFX_InGameMenu_Open.ogg')
click('Таймер')
click('Звуки')
check('leaving sound settings stops preview and remounts a clean table', disabled('■ Остановить'))
check('audio checks do not start or modify the timer history', state()['timer']['status'] == 'Idle' and not state()['sessions'] and not state()['breaks'])
click('Система')
check('settings expose local automatic statistics export', find(app, lambda o: o.get_name() == 'Автовыгрузка для Обсидиана') is not None)
if args.x11:
    export_vault.mkdir()
    select_export_folder(cancel=True)
    check('cancelling the native folder picker does not enable export', not (args.data / 'obsidian.json').exists() and not export_path.exists())
    select_export_folder()
    check('native folder selection saves its own local configuration', (args.data / 'obsidian.json').exists() and json.loads((args.data / 'obsidian.json').read_text())['folder'] == str(export_vault.resolve()))
else:
    previous_config = (args.data / 'obsidian.json').read_bytes()
    select_export_folder(cancel=True)
    check('cancelling the Wayland native folder picker preserves the configured destination', (args.data / 'obsidian.json').read_bytes() == previous_config)
check('automatic export produces a versioned Linux snapshot without private settings', exported(lambda d: d['format'] == 'miku-focus-statistics' and d['schemaVersion'] == 1 and d['source']['os'] == 'linux' and 'settings' not in d and d['sessions'] == []))
peer_export = export_path.with_name('windows.json')
peer_export.write_text('{"fixture":"other OS data, do not change"}')
personal_note = export_vault / 'Мои заметки.md'
personal_note.write_text('Не менять мои записи')
source_id = json.loads(export_path.read_text())['source']['id']
if args.x11:
    click('Отключить автовыгрузку')
    check('disconnect preserves exported files and disables the destination', export_path.exists() and json.loads((args.data / 'obsidian.json').read_text())['folder'] is None)
    select_export_folder()
    check('reconnecting keeps the source identity and overwrites instead of duplicating', exported(lambda d: d['source']['id'] == source_id) and json.loads((args.data / 'obsidian.json').read_text())['folder'] == str(export_vault.resolve()) and len(list(export_path.parent.glob('linux*.json'))) == 1)
    captured = json.loads(export_path.read_text())['capturedAt']
    offline = export_vault.with_name('vault-offline')
    export_vault.rename(offline)
    click('Обновить выгрузку')
    until = time.monotonic() + 5
    while time.monotonic() < until and 'История сохранена локально' not in all_text(app): time.sleep(.05)
    check('missing vault reports export failure without recreating it or changing history', 'История сохранена локально' in all_text(app) and not export_vault.exists() and not state()['sessions'] and state()['timer']['status'] == 'Idle')
    offline.rename(export_vault)
    click('Обновить выгрузку')
    check('manual retry updates the export after the vault returns', exported(lambda d: d['capturedAt'] > captured))
    screenshot('obsidian-settings.png')
click('Категории')
entry('Название категории', 123)
check('category edits autosave without a confirmation button', persisted(lambda s: s['categories'][0]['name'] == '123'))
check('category changes update the connected export automatically', exported(lambda d: d['categories'][0]['name'] == '123'))
click('♡ Комната фокуса')
category_history = (state()['sessions'], state()['breaks'], state()['timer'])
sprite_layout = []
node = find(app, lambda o: o.get_name() == 'Поговорить с Мику')
for _ in range(4):
    if node is None: break
    rect = node.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
    sprite_layout.append({'role': node.get_role_name(), 'name': node.get_name(), 'rect': [rect.x, rect.y, rect.width, rect.height]})
    node = node.get_parent()
(args.data / 'sprite-layout.json').write_text(json.dumps(sprite_layout, ensure_ascii=False))
check('main character reaches the top of the scene panel', sprite_layout[0]['rect'][1] == sprite_layout[1]['rect'][1] + 1 and sprite_layout[0]['rect'][3] == sprite_layout[1]['rect'][3] - 2)
if args.x11:
    original_client = client()
    address = original_client['address']
    subprocess.run(['xdotool', 'windowmove', address, '100', '20'], check=True)
    subprocess.run(['xdotool', 'windowsize', address, '700', '760'], check=True)
    time.sleep(.3)
    character = find(app, lambda o: o.get_name() == 'Поговорить с Мику')
    panel_rect = character.get_parent().get_component_iface().get_extents(Atspi.CoordType.SCREEN)
    character_rect = character.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
    check('tall window keeps the character at the top across resizing', abs(character_rect.y - panel_rect.y - 1) <= 1 and abs(character_rect.height - panel_rect.height + 2) <= 1)
    screenshot('sprite-tall.png')
    subprocess.run(['xdotool', 'windowsize', address, '760', '570'], check=True)
    subprocess.run(['xdotool', 'windowmove', address, *map(str, original_client['at'])], check=True)
    time.sleep(.3)
click('Выбрать категорию')
category_menu = find(app, lambda o: o.get_name() == 'Категории работы')
check('category picker exposes accessible category buttons instead of a native select', category_menu is not None and find(category_menu, lambda o: o.get_name() == 'Чтение' and o.get_action_iface() is not None) is not None)
if args.x11:
    category = find(category_menu, lambda o: o.get_name() == 'Чтение')
    rect = category.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
    subprocess.run(['xdotool', 'mousemove', str(rect.x + rect.width // 2), str(rect.y + rect.height // 2)], check=True)
    screenshot('category-hover.png')
click('Чтение')
check('category button saves selection and closes its menu', persisted(lambda s: s['selectedCategory'] == 'reading') and find(app, lambda o: o.get_name() == 'Категории работы') is None)
trigger = find(app, lambda o: o.get_name() == 'Выбрать категорию')
keys(trigger, 'Down', 'Up', 'Return')
check('category picker supports keyboard navigation and selection', persisted(lambda s: s['selectedCategory'] == 'programming'))
click('Выбрать категорию')
selected_category = find(app, lambda o: o.get_name() == 'Программирование' and o.get_role_name() == 'radio menu item')
check('reopening category picker retains the selected category', selected_category is not None and selected_category.get_state_set().contains(Atspi.StateType.CHECKED))
keys(trigger, 'Down', 'Escape')
check('Escape dismisses the category menu without changing selection', state()['selectedCategory'] == 'programming' and find(app, lambda o: o.get_name() == 'Категории работы') is None)
check('category selection does not change the timer or history', (state()['sessions'], state()['breaks'], state()['timer']) == category_history)
panel = find(app, lambda o: o.get_name() == 'Таймер работы')
check('duration arrows are inside the timer panel', panel is not None and find(panel, lambda o: o.get_name() == increase) is not None and find(panel, lambda o: o.get_name() == decrease) is not None)
check('main timer has no numeric duration field', find(panel, lambda o: o.get_role_name() == 'entry') is None)
if args.x11:
    button = find(app, lambda o: o.get_name() == increase)
    rect = button.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
    subprocess.run(['xdotool', 'mousemove', str(rect.x + rect.width // 2), str(rect.y + rect.height // 2)], check=True)
    time.sleep(.1)
    screenshot('button-hover.png')
check('duration has no apply button', find(app, lambda o: o.get_name() == 'Применить длительность работы') is None)
check('large idle timer initially shows 10:00', '10:00' in all_text(panel))
click(increase)
check('up arrow saves 11 minutes and changes the large timer to 11:00', persisted(lambda s: s['settings']['focusMinutes'] == 11 and s['timer']['remainingMs'] == 660000) and '11:00' in all_text(panel))
screenshot('duration.png')
click(decrease)
check('down arrow changes the large timer back to 10:00', persisted(lambda s: s['settings']['focusMinutes'] == 10 and s['timer']['remainingMs'] == 600000) and '10:00' in all_text(panel))
burst(increase, 3)
check('rapid arrow clicks each add one minute', persisted(lambda s: s['settings']['focusMinutes'] == 13 and s['timer']['remainingMs'] == 780000))
settings_duration(1)
check('down arrow is disabled at one minute', disabled(decrease))
settings_duration(480)
check('up arrow is disabled at 480 minutes', disabled(increase))
settings_duration(2)
click('▶ Начать работу')
check('Start', state()['timer']['status'] == 'Focus')
check('start uses the autosaved duration', state()['timer']['deadlineMs'] - state()['timer']['segmentStartedAt'] == 120000)
deadline = state()['timer']['deadlineMs']
check('arrows cannot change a running session', disabled(increase) and disabled(decrease))
settings_duration(1)
time.sleep(1.2)
check('next duration from settings preserves the current work deadline', persisted(lambda s: s['settings']['focusMinutes'] == 1 and s['timer']['deadlineMs'] == deadline))
screenshot('focus.png')
click('Ⅱ Пауза')
paused = state()
check('Pause captures actual segment', paused['timer']['status'] == 'Paused' and len(paused['timer']['session']['segments']) == 1)
check('paused session is exported once with exact segments and without pause time', exported(lambda d: d['timerStatus'] == 'Paused' and len(d['sessions']) == 1 and d['sessions'][0]['segments'] == paused['timer']['session']['segments']))
time.sleep(0.3)
click('▶ Продолжить')
click('✓ Завершить')
done = state()
check('Finish stores one completed session', len(done['sessions']) == 1 and done['sessions'][0]['completed'])
check('completion replaces the active export record rather than appending a duplicate', exported(lambda d: d['currentSessionId'] is None and d['sessions'] == done['sessions'] and d['totals']['completedSessions'] == 1))
check('rest starts immediately even with auto-start disabled', done['timer']['phase'] == 'ShortBreak' and done['timer']['status'] == 'Break' and done['timer']['deadlineMs'] is not None)
check('rest has no pause or resume control', find(app, lambda o: o.get_name() in ['Ⅱ Пауза', '▶ Продолжить']) is None)
check('work arrows are disabled during rest', disabled(increase) and disabled(decrease))
time.sleep(2)
check('rest has a dedicated Russian reaction', 'Работа закончена.' in all_text(app))
show_focus()
screenshot('rest.png')
if args.full:
    time.sleep(31)
    check('running break updates the export at the periodic checkpoint', exported(lambda d: 1000 < d['totals']['breakMs'] < 60000 and d['capturedAt'] > done['savedAt'] + 1000 and d['timerStatus'] == 'Break'))
    time.sleep(28)
    check('rest finishes without any launch/resume button', state()['timer']['status'] == 'Completed')
    check('rest is recorded separately from work', state()['breaks'][0]['endMs'] - state()['breaks'][0]['startMs'] == 60000 and state()['sessions'][0]['segments'] == done['sessions'][0]['segments'])
else:
    click('Закончить отдых »')
    check('ending rest does not start work when auto-start is disabled', state()['timer']['status'] == 'Completed')

if args.full:
    show_focus()
    click('▶ Начать работу')
    click('Настройки')
    click('Помощник ↗')
    # Sleep to exercise the *real* Rust deadline, not a test-only shortened timer.
    time.sleep(61)
    # A transition persists automatically at the deadline.
    waiting = state()
    check('real one-minute timer reaches AwaitingDecision', waiting['timer']['status'] == 'AwaitingDecision')
    check('deadline caps actual focus at exactly 60 seconds', sum(s['endMs'] - s['startMs'] for s in waiting['timer']['session']['segments']) == 60_000)
    check('decision export caps work and retains just one current session', exported(lambda d: d['timerStatus'] == 'AwaitingDecision' and len(d['sessions']) == 2 and sum(s['endMs'] - s['startMs'] for s in d['sessions'][-1]['segments']) == 60000))
    original = waiting['timer']['session']['id']
    time.sleep(1)
    check('compact companion does not show the completion question', 'Ты справился, сэмпай' not in all_text(app) and 'Ещё пять минут или заслуженный отдых' not in all_text(app))
    click('Мику: клик — открыть, перетаскивание — переместить')
    time.sleep(.9)
    check('opening companion after deadline returns to focus with the victory question', 'Ты справился, сэмпай' in all_text(app) and 'Ещё пять минут или заслуженный отдых' in all_text(app) and find(app, lambda o: o.get_name() == 'Ещё 5 минут ♡') is not None)
    check('reopened decision has the character and dialogue controls', find(app, lambda o: o.get_name() == 'Поговорить с Мику') is not None and find(app, lambda o: o.get_name() == 'Русские субтитры') is not None)
    screenshot('decision-restored.png')
    click('Помощник ↗')
    check('returning to the companion hides the question again', 'Ещё пять минут или заслуженный отдых' not in all_text(app))
    click('Мику: клик — открыть, перетаскивание — переместить')
    time.sleep(.9)
    check('decision survives repeated window switches without modifying work', state()['timer']['session']['id'] == original and state()['timer']['status'] == 'AwaitingDecision' and 'Ты справился, сэмпай' in all_text(app))
    click('Ещё 5 минут ♡')
    extended = state()
    check('extension stays in original session', extended['timer']['session']['id'] == original and extended['timer']['session']['extensions'] == 1)
    check('extension duration is exactly five minutes', extended['timer']['remainingMs'] == 300_000)
    time.sleep(3)
    check('extension waits on its first line instead of auto advancing', 'А? Ещё пять минут, сэмпай?' in all_text(app))
    click('Продолжить диалог')
    time.sleep(.6)
    check('dialogue click advances to the second extension line', 'Хе-хе! Ты такой старательный.' in all_text(app))
    show_focus()
    click('✓ Завершить')
    check('extension commits one session, not separate sessions', len(state()['sessions']) == 2)
    check('long rest also starts automatically', state()['timer']['status'] == 'Break' and state()['timer']['phase'] == 'LongBreak')
    click('Закончить отдых »')

cycle_before = state()
check('completed work advances the cycle counter', cycle_before['timer']['cycle'] > 0)
click('↺ Сбросить сессии')
cycle_expected = cycle_before['timer'].copy()
cycle_expected['cycle'] = 0
check('cycle reset returns to session one without resetting time or history', persisted(lambda s: s['timer'] == cycle_expected and s['sessions'] == cycle_before['sessions'] and s['breaks'] == cycle_before['breaks']) and f"Сессия 1 / {cycle_before['settings']['sessionsBeforeLong']}" in all_text(app))
check('cycle reset is disabled when already at the first session', disabled('↺ Сбросить сессии'))
click('✧ Дневник занятий')
click('Сегодня')
click('Сегодня')
check('repeated inner diary tab click keeps the Today view available', find(app, lambda o: o.get_name() == 'Сегодня') is not None)
click('Статистика')
check('Statistics DOM rendered', find(app, lambda o: o.get_name() == 'Рабочее время по периодам' and o.get_role_name() == 'heading') is not None)
click('Календарь')
check('Calendar DOM rendered', find(app, lambda o: o.get_name().startswith('Следующий месяц')) is not None)
stress_history = (state()['sessions'], state()['breaks'], state()['timer'])
stress_times = []
for quiet in [True, False]:
    click('Переключить тихий режим')
    assert persisted(lambda s: s['settings']['quietMode'] == quiet)
    tabs = [find(app, lambda o, name=name: o.get_name() == name and o.get_action_iface() is not None) for name in ['Сегодня', 'Статистика', 'Календарь']]
    for _ in range(30):
        for tab in tabs:
            started = time.monotonic()
            if args.x11:
                rect = tab.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
                subprocess.run(['xdotool', 'mousemove', str(rect.x + rect.width // 2), str(rect.y + rect.height // 2)], check=True)
            assert tab.get_action_iface().do_action(0)
            stress_times.append(time.monotonic() - started)
            time.sleep(.015)
    click('Статистика')
    check(f'rapid diary navigation remains responsive (quiet={quiet})', find(app, lambda o: o.get_name() == 'Рабочее время по периодам' and o.get_role_name() == 'heading') is not None)
    click('Календарь')
check('rapid diary navigation preserves the timer and history', (state()['sessions'], state()['breaks'], state()['timer']) == stress_history)
check('native effect loading and playback report no errors during navigation', 'Эффект:' not in all_text(app))
(args.data / 'navigation-stress.json').write_text(json.dumps({'actions': len(stress_times), 'maxActionSeconds': max(stress_times), 'backend': 'X11/Xvfb' if args.x11 else 'Wayland/Hyprland'}))
calendar_before = all_text(find(app, lambda o: o.get_name() == 'Следующий месяц').get_parent())
click('Следующий месяц')
check('calendar next arrow changes the displayed month', all_text(find(app, lambda o: o.get_name() == 'Следующий месяц').get_parent()) != calendar_before)
click('Предыдущий месяц')
check('calendar previous arrow returns to the original month', all_text(find(app, lambda o: o.get_name() == 'Следующий месяц').get_parent()) == calendar_before)
click('Таблица CSV ↓')
click('Данные JSON ↓')
check('CSV and JSON exports exist', len(list((args.data / 'exports').glob('*.csv'))) >= 1 and len(list((args.data / 'exports').glob('*.json'))) >= 1)
check('vault snapshot matches all completed history and separate rest segments', exported(lambda d: d['sessions'] == state()['sessions'] and d['breaks'] == state()['breaks']))
check('export leaves other OS data and handwritten notes untouched', peer_export.read_text() == '{"fixture":"other OS data, do not change"}' and personal_note.read_text() == 'Не менять мои записи')
click('Настройки')
click('Мику и голос')
check('text speed survives leaving and reopening settings', entry_text('Скорость появления символов, симв/с') == '120')
check('removed ornaments and slogans stay absent', all(text not in all_text(app) for text in ['୨୧', 'Немного времени, немного волшебства', 'Маленький уютный мир']))
click('Помощник ↗')
time.sleep(.8)
c = client()
if not args.x11:
    check('native Wayland companion is floating', c['floating'] and not c['xwayland'])
check('compact companion starts at 180x190', c['size'] == [180, 190])
check('companion has a resize grip', find(app, lambda o: o.get_name() == 'Изменить размер помощника') is not None)
history = (state()['sessions'], state()['breaks'])
if args.x11:
    subprocess.run(['xdotool', 'windowmove', c['address'], '111', '222'], check=True)
else:
    hypr_dispatch(args.pid, c['address'], 'move', 111, 222)
geometry_settled(lambda c: c['at'] == [111, 222])
if args.pointer_helper:
    c = client()
    monitors = json.loads(subprocess.check_output(['hyprctl', '-j', 'monitors'], text=True))
    assert len(monitors) == 1, 'Pointer smoke expects one monitor; do not guess global coordinates'
    monitor = monitors[0]
    scale = monitor.get('scale', 1)
    width, height = int(monitor['width'] / scale), int(monitor['height'] / scale)
    hypr_dispatch(args.pid, c['address'], 'focus')
    subprocess.run([str(args.pointer_helper.resolve()), str(c['at'][0] + c['size'][0] - 10), str(c['at'][1] + c['size'][1] - 10), str(width), str(height), 'drag'], check=True)
    geometry_settled(lambda c: abs(c['size'][0] - 245) <= 1 and abs(c['size'][1] - 235) <= 1 and c['at'] == [111, 222])
    time.sleep(.4)  # Virtual absolute-pointer coordinates may round by one logical pixel.
    mouse_size = client()['size']
    check('real Wayland pointer resizes the widget by its corner', persisted(lambda s: s['settings']['companionSize'] == mouse_size))
    check('corner resize keeps the top-left position fixed', client()['at'] == [111, 222])
    screenshot('companion-mouse.png')

before_size = client()['size']
grip = find(app, lambda o: o.get_name() == 'Изменить размер помощника')
address = client()['address']
if args.x11:
    subprocess.run(['xdotool', 'windowfocus', '--sync', address], check=True)
else:
    hypr_dispatch(args.pid, address, 'focus')
assert grip.get_component_iface().grab_focus()
for key, expected in [('Right', [before_size[0] + 10, before_size[1]]), ('Down', [before_size[0] + 10, before_size[1] + 10])]:
    if args.x11:
        subprocess.run(['xdotool', 'key', '--clearmodifiers', key], check=True)
    else:
        hypr_dispatch(args.pid, address, 'shortcut', '', key)
    geometry_settled(lambda c: c['size'] == expected)
expected_size = [before_size[0] + 10, before_size[1] + 10]
check('resize grip supports keyboard resizing and saves logical dimensions', persisted(lambda s: s['settings']['companionSize'] == expected_size))
check('resizing does not change focus or rest history', (state()['sessions'], state()['breaks']) == history)
screenshot('companion-resized.png')
click('Мику: клик — открыть, перетаскивание — переместить')
check('companion position persists', state()['settings']['companionPosition'] == [111, 222])
click('Помощник ↗')
geometry_settled(lambda c: c['at'] == [111, 222])
check('companion position restores', client()['at'] == [111, 222])
check('companion size restores after switching windows', client()['size'] == expected_size)
click('Мику: клик — открыть, перетаскивание — переместить')
finish_exit_checks()
args.data.parent.mkdir(parents=True, exist_ok=True)
(args.data.parent / 'smoke-results.json').write_text(json.dumps({'backend': 'X11/Xvfb' if args.x11 else 'Wayland/Hyprland', 'checks': checks, 'count': len(checks)}, indent=2))
print(f'Native smoke passed: {len(checks)} checks', flush=True)
