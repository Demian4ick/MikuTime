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

p = argparse.ArgumentParser()
p.add_argument('--pid', type=int, required=True)
p.add_argument('--data', type=Path, required=True)
p.add_argument('--full', action='store_true', help='Wait for a real one-minute focus deadline')
p.add_argument('--x11', action='store_true', help='Private Xvfb/DBus session; not a Wayland verification')
args = p.parse_args()
if '.local-test' not in args.data.resolve().parts:
    raise SystemExit('Refusing to run outside .local-test: do not mutate personal history')
root = Atspi.get_desktop(0)
app = next(root.get_child_at_index(i) for i in range(root.get_child_count())
           if root.get_child_at_index(i).get_process_id() == args.pid)
checks = []

def find(obj, predicate):
    try:
        if predicate(obj): return obj
        for i in range(obj.get_child_count()):
            match = find(obj.get_child_at_index(i), predicate)
            if match: return match
    except GLib.Error: pass
    return None

def click(name):
    obj = find(app, lambda o: o.get_name() == name and o.get_action_iface() is not None)
    assert obj is not None, f'Control not found: {name}'
    assert obj.get_action_iface().do_action(0), name
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
        window = subprocess.check_output(['xdotool', 'search', '--onlyvisible', '--pid', str(args.pid), '--name', '^Miku Focus'], text=True).splitlines()[0]
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
    subprocess.run(['hyprctl', 'dispatch', 'focuswindow', 'address:' + address], check=True, capture_output=True)
    assert obj.get_component_iface().grab_focus()
    # Physical A key: independent of the user's active EN/RU keyboard layout.
    for mods, key in [('CTRL', 'code:38'), *[('', char) for char in str(value)]]:
        out = subprocess.check_output(['hyprctl', 'dispatch', 'sendshortcut', f'{mods},{key},address:{address}'], text=True)
        assert out.strip() == 'ok', out
        time.sleep(0.06)

def entry_text(prefix):
    obj = find(app, lambda o: o.get_role_name() == 'entry' and o.get_name().startswith(prefix))
    assert obj, prefix
    return Atspi.Text.get_text(obj, 0, -1)

def state(): return json.loads((args.data / 'state.json').read_text())
def check(name, condition):
    assert condition, name
    checks.append(name)
    print('PASS', name, flush=True)

def persisted(predicate):
    until = time.monotonic() + 5
    while time.monotonic() < until:
        if predicate(state()): return True
        time.sleep(.05)
    return False

def geometry_settled(predicate):
    # Hyprland IPC exposes interpolated geometry while the user's animations run.
    until = time.monotonic() + 5
    while time.monotonic() < until:
        c = client()
        if predicate(c): return c
        time.sleep(.15)
    raise AssertionError('Compositor geometry did not settle')

def screenshot(name):
    if args.x11: return
    c = client()
    subprocess.run(['hyprctl', 'dispatch', 'focuswindow', 'address:' + c['address']], capture_output=True, check=True)
    time.sleep(.3)
    c = client(); x, y = c['at']; w, h = c['size']
    subprocess.run(['grim', '-g', f'{x},{y} {w}x{h}', str(args.data.parent / name)], check=True)

click('Настройки')
click('Таймер')
entry('Работа, мин', 10)
entry('Короткий отдых, мин', 1)
entry('Длинный отдых, мин', 1)
entry('Сессий перед длинным отдыхом', 2)
check('break settings autosave through real DOM input and IPC', persisted(lambda s: s['settings']['shortBreakMinutes'] == 1 and s['settings']['longBreakMinutes'] == 1 and s['settings']['sessionsBeforeLong'] == 2))
check('settings have no save button', find(app, lambda o: o.get_name() == 'Сохранить ♡') is None)
click('Мику и голос')
check('legacy-compatible default text speed is available', entry_text('Скорость появления символов, симв/с') == '60')
entry('Скорость появления символов, симв/с', 0)
check('instant text speed autosaves', persisted(lambda s: s['settings']['textSpeed'] == 0))
entry('Скорость появления символов, симв/с', 120)
check('custom text speed autosaves', persisted(lambda s: s['settings']['textSpeed'] == 120))
screenshot('settings.png')
click('Категории')
entry('Название категории', 123)
check('category edits autosave without a confirmation button', persisted(lambda s: s['categories'][0]['name'] == '123'))
click('♡ Комната фокуса')
panel = find(app, lambda o: o.get_name() == 'Таймер работы')
check('duration arrows are inside the timer panel', panel is not None and find(panel, lambda o: o.get_name() == increase) is not None and find(panel, lambda o: o.get_name() == decrease) is not None)
check('main timer has no numeric duration field', find(panel, lambda o: o.get_role_name() == 'entry') is None)
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
time.sleep(0.3)
click('▶ Продолжить')
click('✓ Завершить')
done = state()
check('Finish stores one completed session', len(done['sessions']) == 1 and done['sessions'][0]['completed'])
check('rest starts immediately even with auto-start disabled', done['timer']['phase'] == 'ShortBreak' and done['timer']['status'] == 'Break' and done['timer']['deadlineMs'] is not None)
check('rest has no pause or resume control', find(app, lambda o: o.get_name() in ['Ⅱ Пауза', '▶ Продолжить']) is None)
check('work arrows are disabled during rest', disabled(increase) and disabled(decrease))
time.sleep(2)
check('rest has a dedicated Russian reaction', 'Работа закончена, сэмпай' in all_text(app))
show_focus()
screenshot('rest.png')
if args.full:
    time.sleep(59)
    check('rest finishes without any launch/resume button', state()['timer']['status'] == 'Completed')
    check('rest is recorded separately from work', state()['breaks'][0]['endMs'] - state()['breaks'][0]['startMs'] == 60000 and state()['sessions'][0]['segments'] == done['sessions'][0]['segments'])
else:
    click('Закончить отдых »')
    check('ending rest does not start work when auto-start is disabled', state()['timer']['status'] == 'Completed')

if args.full:
    show_focus()
    click('▶ Начать работу')
    # Sleep to exercise the *real* Rust deadline, not a test-only shortened timer.
    time.sleep(61)
    # A transition persists automatically at the deadline.
    waiting = state()
    check('real one-minute timer reaches AwaitingDecision', waiting['timer']['status'] == 'AwaitingDecision')
    check('deadline caps actual focus at exactly 60 seconds', sum(s['endMs'] - s['startMs'] for s in waiting['timer']['session']['segments']) == 60_000)
    original = waiting['timer']['session']['id']
    time.sleep(1)
    show_focus()
    click('Ещё 5 минут ♡')
    extended = state()
    check('extension stays in original session', extended['timer']['session']['id'] == original and extended['timer']['session']['extensions'] == 1)
    check('extension duration is exactly five minutes', extended['timer']['remainingMs'] == 300_000)
    time.sleep(5)
    show_focus()
    click('✓ Завершить')
    check('extension commits one session, not separate sessions', len(state()['sessions']) == 2)
    check('long rest also starts automatically', state()['timer']['status'] == 'Break' and state()['timer']['phase'] == 'LongBreak')
    click('Закончить отдых »')

click('✧ Дневник занятий')
click('Статистика')
check('Statistics DOM rendered', find(app, lambda o: o.get_name() == 'Рабочее время по периодам' and o.get_role_name() == 'heading') is not None)
click('Календарь')
check('Calendar DOM rendered', find(app, lambda o: o.get_name().startswith('Следующий месяц')) is not None)
click('Таблица CSV ↓')
click('Данные JSON ↓')
check('CSV and JSON exports exist', len(list((args.data / 'exports').glob('*.csv'))) >= 1 and len(list((args.data / 'exports').glob('*.json'))) >= 1)
click('Настройки')
click('Мику и голос')
check('text speed survives leaving and reopening settings', entry_text('Скорость появления символов, симв/с') == '120')
check('removed ornaments and slogans stay absent', all(text not in all_text(app) for text in ['୨୧', 'Немного времени, немного волшебства', 'Маленький уютный мир']))
click('Помощник ↗')
time.sleep(.8)
c = client()
if not args.x11:
    check('native Wayland companion is floating', c['floating'] and not c['xwayland'])
check('companion geometry is 300x360', c['size'] == [300, 360])
if args.x11:
    subprocess.run(['xdotool', 'windowmove', c['address'], '111', '222'], check=True)
else:
    subprocess.run(['hyprctl', 'dispatch', 'movewindowpixel', f'exact 111 222,address:{c["address"]}'], check=True, capture_output=True)
geometry_settled(lambda c: c['at'] == [111, 222])
click('Мику: клик — открыть, перетаскивание — переместить')
check('companion position persists', state()['settings']['companionPosition'] == [111, 222])
click('Помощник ↗')
geometry_settled(lambda c: c['at'] == [111, 222])
check('companion position restores', client()['at'] == [111, 222])
click('Мику: клик — открыть, перетаскивание — переместить')
click('Сохранить и выйти')
args.data.parent.mkdir(parents=True, exist_ok=True)
(args.data.parent / 'smoke-results.json').write_text(json.dumps({'backend': 'X11/Xvfb' if args.x11 else 'Wayland/Hyprland', 'checks': checks, 'count': len(checks)}, indent=2))
print(f'Native smoke passed: {len(checks)} checks', flush=True)
