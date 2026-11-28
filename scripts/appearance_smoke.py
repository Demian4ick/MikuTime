"""Private real-WebKit checks for appearance controls. No user profile or desktop access."""
import hashlib
import json
from pathlib import Path
import stat
import subprocess
import time
from PIL import Image, ImageDraw


def seed(data):
    assert '.local-test' in data.resolve().parts
    assets = data / 'assets'; assets.mkdir(exist_ok=True)
    old = assets / 'old-background.png'
    Image.new('RGB', (160, 90), '#6aa898').save(old)
    path = data / 'state.json'; state = json.loads(path.read_text())
    state['settings']['background'] = str(old)
    path.write_text(json.dumps(state))


def run(args, app, *, click, find, keys, state, persisted, check, all_text, screenshot):
    from gi.repository import Atspi

    def appearance():
        click('Настройки'); click('Оформление')

    def dark_frame(name):
        screenshot(name)
        image = Image.open(args.data.parent / name).convert('RGB')
        pixel = image.getpixel((image.width - 10, image.height - 8))
        check(name + ': dark surface is actually rendered', max(pixel) < 120)

    if args.appearance_reload:
        check('dark theme is retained after a real process restart', state()['settings']['theme'] == 'Dark')
        appearance()
        check('gallery selection survives a real process restart', state()['settings']['background'] == state()['settings']['wallpapers'][0]['path'] and len(state()['settings']['wallpapers']) == 1)
        dark_frame('dark-reload.png')
        click('Светлая'); assert persisted(lambda s: s['settings']['theme'] == 'Light')
        screenshot('light-reload.png')
        image = Image.open(args.data.parent / 'light-reload.png').convert('RGB')
        check('returning to light restores the light surface', min(image.getpixel((image.width - 10, image.height - 8))) > 170)
        click('Сохранить и выйти'); return

    before = state()
    appearance()
    check('legacy background is migrated once without changing the selected file', len(state()['settings']['wallpapers']) == 1 and state()['settings']['background'] == before['settings']['background'])
    click('Тёмная'); assert persisted(lambda s: s['settings']['theme'] == 'Dark')
    dark_frame('dark-appearance.png')
    click('♡ Комната фокуса'); dark_frame('dark-focus.png')
    click('✧ Дневник занятий'); dark_frame('dark-diary.png')
    click('Календарь'); dark_frame('dark-calendar.png')
    click('Настройки'); click('Звуки'); dark_frame('dark-sounds.png')
    click('Мику и голос'); click('Редактировать JSON диалогов'); dark_frame('dark-dialogue-editor.png')
    keys(find(app, lambda o: o.get_name() == 'JSON диалогов'), 'Escape')
    appearance()

    folder = args.data.parent / 'wallpaper-fixtures'; folder.mkdir()
    for name, color, extension in [('Лес', '#508e80', 'png'), ('Город', '#65587c', 'jpg')]:
        image = Image.new('RGB', (1280, 720), color)
        ImageDraw.Draw(image).rectangle((100, 100, 900, 500), outline='#d6efda', width=12)
        image.save(folder / f'{name}.{extension}')
    originals = {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in folder.iterdir()}

    # Select two real files in GTK via its own accessibility selection interface.
    click('Добавить обои')
    chooser = find(app, lambda o: o.get_role_name() == 'file chooser')
    assert chooser is not None
    windows = subprocess.check_output(['xdotool', 'search', '--all', '--onlyvisible', '--pid', str(args.pid)], text=True).splitlines()
    window_info = []
    for window in windows:
        title = subprocess.check_output(['xdotool', 'getwindowname', window], text=True).strip()
        window_info.append(window + ' ' + title + '\n' + subprocess.check_output(['xdotool', 'getwindowgeometry', '--shell', window], text=True))
        if title and title != 'Miku Focus' and 'помощник' not in title:
            subprocess.run(['xdotool', 'windowsize', window, '1000', '700'], check=True)
            subprocess.run(['xdotool', 'windowmove', window, '10', '10'], check=True)
            subprocess.run(['xdotool', 'windowfocus', '--sync', window], check=True)
            break
    (args.data.parent / 'chooser-windows.txt').write_text('\n'.join(window_info))
    time.sleep(.2)
    home = find(chooser, lambda o: o.get_name() == 'Домашняя папка' and o.get_state_set().contains(Atspi.StateType.SHOWING))
    if home is not None:
        rect = home.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
        subprocess.run(['xdotool', 'mousemove', str(rect.x + rect.width // 2), str(rect.y + rect.height // 2), 'click', '1'], check=True)
        time.sleep(.25)
    subprocess.run(['xdotool', 'key', '--clearmodifiers', 'ctrl+l'], check=True)
    time.sleep(.3)
    location = find(chooser, lambda o: o.get_editable_text_iface() is not None and o.get_state_set().contains(Atspi.StateType.FOCUSED))
    if location is not None:
        assert location.get_editable_text_iface().set_text_contents(str(folder.resolve()))
    else:
        subprocess.run(['xdotool', 'type', '--clearmodifiers', '--delay', '10', str(folder.resolve())], check=True)
    subprocess.run(['xdotool', 'key', '--clearmodifiers', 'Return'], check=True)
    time.sleep(1)
    chooser.clear_cache()
    cell = find(chooser, lambda o: 'Лес.png' in o.get_name())
    if cell is None:
        open_folder = find(chooser, lambda o: o.get_name().lstrip('_') in ['Открыть', 'Open'] and o.get_action_iface() is not None)
        assert open_folder is not None and open_folder.get_action_iface().do_action(0)
        time.sleep(1); chooser.clear_cache()
        cell = find(chooser, lambda o: 'Лес.png' in o.get_name())
    if cell is None:
        def tree(obj, depth=0):
            result = [('  ' * depth) + obj.get_role_name() + ': ' + obj.get_name() + (' ' + str(obj.get_attributes()) if obj.get_editable_text_iface() is not None else '')]
            for index in range(obj.get_child_count()):
                child = obj.get_child_at_index(index)
                if child is not None: result.extend(tree(child, depth + 1))
            return result
        (args.data.parent / 'chooser-tree.txt').write_text('\n'.join(tree(chooser)) + '\nTEXT:\n' + all_text(chooser) + '\nFOCUS:\n' + subprocess.check_output(['xdotool', 'getwindowfocus', 'getwindowname'], text=True))
        screenshot('appearance-chooser-failure.png')
    assert cell is not None, 'The owned file chooser did not open the private fixture folder'
    table = find(chooser, lambda o: o.get_role_name() == 'table' and o.get_name() == 'Файлы')
    assert table is not None
    if table.get_selection_iface() is None or not table.get_selection_iface().select_all():
        rect = cell.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
        subprocess.run(['xdotool', 'mousemove', str(rect.x + rect.width // 2), str(rect.y + rect.height // 2), 'click', '1'], check=True)
        subprocess.run(['xdotool', 'key', '--clearmodifiers', 'ctrl+a'], check=True)
        time.sleep(.2)
    confirm = find(chooser, lambda o: o.get_name().lstrip('_') in ['Открыть', 'Open'] and o.get_action_iface() is not None)
    assert confirm is not None and confirm.get_action_iface().do_action(0)
    check('one native chooser imports multiple wallpapers', persisted(lambda s: len(s['settings']['wallpapers']) == 3))
    items = state()['settings']['wallpapers'][1:]
    check('imports preserve PNG and JPEG originals byte for byte', all(hashlib.sha256(Path(item['path']).read_bytes()).hexdigest() == originals[next(name for name in originals if name.rsplit('.', 1)[0] == item['name'])] for item in items))
    check('gallery uses small saved previews', all(Image.open(item['thumbnail']).size == (320, 180) and Path(item['thumbnail']).stat().st_size < 256 * 1024 for item in items))
    forest = next(item for item in items if item['name'] == 'Лес')
    city = next(item for item in items if item['name'] == 'Город')
    click('Выбрать обои Лес')
    check('wallpaper card selects the stored image', persisted(lambda s: s['settings']['background'] == forest['path']))
    dark_frame('dark-gallery.png')
    mode = stat.S_IMODE(args.data.stat().st_mode)
    try:
        args.data.chmod(0o500)
        click('Светлая')
        check('failed theme save keeps the saved theme', state()['settings']['theme'] == 'Dark')
        click('Выбрать стандартные обои')
        check('failed selection does not discard the saved wallpaper', state()['settings']['background'] == forest['path'])
    finally:
        args.data.chmod(mode)
    click('Светлая'); assert persisted(lambda s: s['settings']['theme'] == 'Light')
    click('Тёмная'); assert persisted(lambda s: s['settings']['theme'] == 'Dark')
    click('Выбрать стандартные обои')
    check('standard wallpaper remains available', persisted(lambda s: s['settings']['background'] is None))
    click('Выбрать обои Лес')
    click('Убрать обои Город'); click('Отмена')
    check('cancelling removal keeps the gallery entry', len(state()['settings']['wallpapers']) == 3)
    click('Убрать обои Город'); click('Убрать')
    check('removing an inactive wallpaper keeps the current choice', persisted(lambda s: len(s['settings']['wallpapers']) == 2 and s['settings']['background'] == forest['path']))
    click('Убрать обои Лес'); click('Убрать')
    check('removing the active wallpaper returns to the default', persisted(lambda s: len(s['settings']['wallpapers']) == 1 and s['settings']['background'] is None))
    check('removing gallery entries never deletes retained copies', all(Path(item['path']).exists() and Path(item['thumbnail']).exists() for item in items))
    check('source image files are never changed', all(hashlib.sha256((folder / name).read_bytes()).hexdigest() == digest for name, digest in originals.items()))

    click('Выбрать обои Свой фон')
    assert persisted(lambda s: s['settings']['background'] == s['settings']['wallpapers'][0]['path'])
    invalid = folder / 'corrupt.png'
    invalid.write_bytes((folder / 'Лес.png').read_bytes()[:33])
    click('Добавить обои')
    invalid_chooser = find(app, lambda o: o.get_role_name() == 'file chooser')
    assert invalid_chooser is not None
    owned_dialog = subprocess.check_output(['xdotool', 'search', '--all', '--onlyvisible', '--pid', str(args.pid), '--name', '^Select Files$'], text=True).splitlines()[0]
    subprocess.run(['xdotool', 'windowfocus', '--sync', owned_dialog], check=True)
    subprocess.run(['xdotool', 'key', '--clearmodifiers', 'ctrl+l'], check=True)
    time.sleep(.2)
    location = find(invalid_chooser, lambda o: o.get_editable_text_iface() is not None and o.get_state_set().contains(Atspi.StateType.FOCUSED))
    assert location is not None and location.get_editable_text_iface().set_text_contents(str(invalid.resolve()))
    assert location.get_component_iface().grab_focus()
    subprocess.run(['xdotool', 'key', '--window', owned_dialog, '--clearmodifiers', 'Return'], check=True)
    time.sleep(.4)
    if find(app, lambda o: o.get_role_name() == 'file chooser' and o.get_state_set().contains(Atspi.StateType.SHOWING)) is not None:
        open_invalid = find(invalid_chooser, lambda o: o.get_name().lstrip('_') in ['Открыть', 'Open'] and o.get_action_iface() is not None)
        assert open_invalid is not None and open_invalid.get_action_iface().do_action(0)
    time.sleep(.6); app.clear_cache()
    check('invalid image chooser closes before validation', find(app, lambda o: o.get_role_name() == 'file chooser' and o.get_state_set().contains(Atspi.StateType.SHOWING)) is None)
    check('invalid images do not create a gallery entry', len(state()['settings']['wallpapers']) == 1)
    check('invalid image reports a readable error', 'Не удалось прочитать изображение' in all_text(app))
    after = state()
    check('appearance changes preserve timer and recorded history', all(before[key] == after[key] for key in ['timer', 'sessions', 'breaks', 'categories']))
    check('appearance changes preserve audio bindings and volume', all(before['settings'].get(key) == after['settings'].get(key) for key in ['voices', 'sfx', 'sfxBindings', 'volume', 'sfxVolume']))
    click('Помощник ↗')
    check('dark companion remains available', find(app, lambda o: o.get_name() == 'Мику: клик — открыть, перетаскивание — переместить') is not None)
    screenshot('dark-companion.png')
    click('Мику: клик — открыть, перетаскивание — переместить')
    time.sleep(.5); app.clear_cache()
    main = subprocess.check_output(['xdotool', 'search', '--all', '--onlyvisible', '--pid', str(args.pid), '--name', '^Miku Focus$'], text=True).splitlines()[0]
    subprocess.run(['xdotool', 'windowfocus', '--sync', main], check=True)
    click('Сохранить и выйти')
    quitting = find(app, lambda o: o.get_name() == 'Завершение работы') is not None
    if not quitting:
        (args.data.parent / 'appearance-quit-text.txt').write_text(all_text(app))
        screenshot('appearance-quit-failure.png')
    check('appearance queues allow a normal save-and-exit', quitting)
    time.sleep(4)
    process = Path(f'/proc/{args.pid}/stat')
    if process.exists() and process.read_text().split(') ', 1)[1].split()[0] != 'Z':
        app.clear_cache()
        (args.data.parent / 'appearance-after-quit.txt').write_text(all_text(app))
        screenshot('appearance-after-quit.png')
