"""Addressed runtime dispatch only, for the explicitly owned native-test PID."""
import json
import re
import subprocess
from functools import cache


@cache
def lua_dispatchers():
    result = subprocess.run(['hyprctl', 'repl', 'return type(hl.dsp)'], capture_output=True, text=True, timeout=5)
    return result.returncode == 0 and result.stdout.strip() == 'table'


def dispatch(pid, address, action, *values):
    assert re.fullmatch(r'0x[0-9a-fA-F]+', address), 'Invalid owned address'
    clients = json.loads(subprocess.check_output(['hyprctl', '-j', 'clients'], text=True))
    assert any(c['pid'] == pid and c['address'] == address for c in clients), 'Test window no longer belongs to this PID'
    if lua_dispatchers():
        if action == 'focus': expression = 'hl.dsp.focus({window=w})'
        elif action == 'shortcut':
            mods, key = values
            expression = f'hl.dsp.send_shortcut({{window=w,mods={json.dumps(mods)},key={json.dumps(key)}}})'
        elif action == 'move':
            x, y = map(int, values)
            expression = f'hl.dsp.window.move({{window=w,x={x},y={y},relative=false}})'
        else: raise AssertionError(f'Unsupported test action: {action}')
        command = ['hyprctl', 'dispatch', f'function() local w=hl.get_window("address:{address}"); assert(w and w.pid=={int(pid)},"Owned test window unavailable"); hl.dispatch({expression}) end']
    else:
        if action == 'focus': name, argument = 'focuswindow', f'address:{address}'
        elif action == 'shortcut': name, argument = 'sendshortcut', f'{values[0]},{values[1]},address:{address}'
        elif action == 'move': name, argument = 'movewindowpixel', f'exact {int(values[0])} {int(values[1])},address:{address}'
        else: raise AssertionError(f'Unsupported test action: {action}')
        command = ['hyprctl', 'dispatch', name, argument]
    result = subprocess.run(command, capture_output=True, text=True, timeout=5)
    assert result.returncode == 0 and result.stdout.strip() == 'ok', f'Owned {action} failed: {result.stdout} {result.stderr}'
