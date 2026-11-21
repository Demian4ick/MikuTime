"""Measure only an owned app's PCM on a temporary null sink; never record the microphone/default monitor."""
import argparse
from array import array
import json
import hashlib
import math
import os
from pathlib import Path
import subprocess
import time
import wave
import gi
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi, GLib
from smoke_fixture import seed_silent_data

p = argparse.ArgumentParser()
p.add_argument('--debug', action='store_true')
p.add_argument('--binary', type=Path, help='Existing owned baseline binary; still uses private app-data and sink')
p.add_argument('--decision', action='store_true', help='Use a private waiting-decision checkpoint and a supplied UI WAV as a voice probe, not real character speech')
p.add_argument('--panel', action='store_true', help='With --decision, check fully hidden panel mode instead of the companion')
args = p.parse_args()
if args.panel and not args.decision: p.error('--panel requires --decision')
root = Path(__file__).resolve().parent.parent
binary = args.binary.resolve() if args.binary else root / f'src-tauri/target/{"debug" if args.debug else "release"}/miku-focus'
base = root / '.local-test' / f'audio-output-{time.time_ns()}'
base.mkdir(mode=0o700)
data = base / 'data/dev.mikufocus.desktop'
seed_silent_data(data)
state_path = data / 'state.json'
state = json.loads(state_path.read_text())
state['settings'].update(sfxVolume=0.5, volume=0.5, quietMode=False)
if args.decision:
    asset = data / 'assets/voices/question.wav'
    asset.parent.mkdir(parents=True)
    asset.write_bytes((root / 'sfx/menu-open.wav').read_bytes())
    state['settings'].update(sfxVolume=0, sfxBindings={}, soundDefaultsVersion=2, voices={'focus_complete': str(asset)})
    if args.panel: state['settings']['backgroundMode'] = 'Panel'
    now = int(time.time() * 1000)
    state['savedAt'] = now
    state['timer'].update(status='AwaitingDecision', remainingMs=0, session={
        'id': 'decision-fixture', 'categoryId': 'programming', 'startedAt': now - 60000,
        'endedAt': None, 'segments': [{'startMs': now - 60000, 'endMs': now}], 'extensions': 0, 'completed': False,
    })
state_path.write_text(json.dumps(state))
sink = f'miku_test_{os.getpid()}'
default_sink = subprocess.check_output(['pactl', 'get-default-sink'], text=True).strip()
module = subprocess.check_output(['pactl', 'load-module', 'module-null-sink', f'sink_name={sink}', 'rate=48000', 'channels=2'], text=True).strip()
assert module.isdecimal()
app = recorder = None
results = {}
passed = False
print('Audio test directory:', base, flush=True)

def find(obj, name):
    try:
        if obj.get_name() == name and obj.get_action_iface() is not None: return obj
        count = obj.get_child_count()
    except GLib.Error: return None
    for i in range(count):
        try: child = obj.get_child_at_index(i)
        except GLib.Error: continue
        if child is not None:
            result = find(child, name)
            if result is not None: return result
    return None

def click(name):
    control = find(accessible, name)
    assert control is not None and control.get_action_iface().do_action(0), name
    time.sleep(.2)

def visible_text(obj):
    try:
        value = Atspi.Text.get_text(obj, 0, -1) if obj.get_text_iface() else ''
        for i in range(obj.get_child_count()):
            child = obj.get_child_at_index(i)
            if child is not None: value += visible_text(child)
        return value
    except GLib.Error: return ''

def capture(name, action):
    global recorder
    path = base / f'{name}.wav'
    raw = base / f'{name}.raw'
    # Raw parec avoids FFmpeg timestamp/probing drops around tiny, isolated 60ms signals.
    # Require actual PCM on disk, not merely a connected recorder, before the click.
    with raw.open('wb') as output:
        recorder_env = dict(os.environ, PULSE_PROP='application.name=miku-audio-test-recorder module-stream-restore.id=miku-audio-test-recorder')
        recorder = subprocess.Popen(['parec', '--raw', '--format=s16le', '--channels=1', '--rate=48000', '--latency-msec=30', '--process-time-msec=10', '--device=' + sink + '.monitor'], env=recorder_env, stdout=output, stderr=log)
        until = time.monotonic() + 5
        while time.monotonic() < until:
            outputs = json.loads(subprocess.check_output(['pactl', '-f', 'json', 'list', 'source-outputs']))
            if raw.stat().st_size >= 9600 and any(o.get('properties', {}).get('application.process.id') == str(recorder.pid) for o in outputs): break
            assert recorder.poll() is None, 'Recorder exited before connecting'
            time.sleep(.05)
        else: raise RuntimeError('Owned null-monitor recorder did not deliver PCM')
        action()
        # Cold opening from a fully unmapped panel can take longer than reopening
        # an already mapped companion. Keep recording through that transition.
        time.sleep(3 if args.panel else 1)
        recorder.terminate(); recorder.wait(timeout=10); recorder = None
    pcm = raw.read_bytes()
    with wave.open(str(path), 'wb') as audio:
        audio.setnchannels(1); audio.setsampwidth(2); audio.setframerate(48000); audio.writeframes(pcm)
    samples = array('h', pcm)
    energy = sum(x*x for x in samples)
    result = {'peak': max(map(abs, samples), default=0), 'rms': math.sqrt(energy / max(1, len(samples))), 'energy': energy, 'samples': len(samples), 'nonzero': sum(x != 0 for x in samples)}
    results[name] = result
    (base / f'{name}-ui.txt').write_text(visible_text(accessible))
    print(name, json.dumps(result), flush=True)
    (base / 'results.json').write_text(json.dumps(results, indent=2))

with (base / 'desktop.log').open('w') as log:
    try:
        env = dict(os.environ, GDK_BACKEND='wayland', XDG_DATA_HOME=str(base / 'data'), XDG_CONFIG_HOME=str(base / 'config'), XDG_CACHE_HOME=str(base / 'cache'), PULSE_SINK=sink, PULSE_PROP='application.name=miku-audio-test module-stream-restore.id=miku-audio-test')
        assert os.environ.get('WAYLAND_DISPLAY'), 'Run from Wayland; this driver does not claim X11 verification'
        app = subprocess.Popen([str(binary), *(['--companion'] if args.panel else [])], env=env, stdout=log, stderr=log)
        print('Owned test PID:', app.pid, flush=True)
        for _ in range(100):
            assert app.poll() is None, 'Owned app exited'
            desktop = Atspi.get_desktop(0)
            accessible = next((desktop.get_child_at_index(i) for i in range(desktop.get_child_count()) if desktop.get_child_at_index(i).get_process_id() == app.pid), None)
            if accessible is not None: break
            time.sleep(.1)
        else: raise RuntimeError('Owned app not found on AT-SPI')
        time.sleep(1)
        if args.decision:
            background = 'panel' if args.panel else 'companion'
            def open_main():
                if args.panel: subprocess.run([str(binary), '--show-main'], env=env, check=True, timeout=8)
                else: click('Мику: клик — открыть, перетаскивание — переместить')
            def assert_hidden():
                clients = json.loads(subprocess.check_output(['hyprctl', '-j', 'clients']))
                assert not any(c['pid'] == app.pid for c in clients), 'Panel mode left an owned window mapped'
            def hide_main():
                click('Свернуть в панель ↗' if args.panel else 'Помощник ↗')
                if args.panel: assert_hidden()
            capture(f'decision-{background}-silent', assert_hidden if args.panel else hide_main)
            capture('decision-main-voice', open_main)
            capture(f'decision-{background}-silent-again', hide_main)
            capture('decision-main-voice-again', open_main)
            click('Ещё 5 минут ♡'); hide_main()
            capture('decision-not-restored-during-extension', open_main)
            for name in [f'decision-{background}-silent', f'decision-{background}-silent-again', 'decision-not-restored-during-extension']:
                assert results[name]['energy'] == 0, f'Unexpected question voice: {name}'
            assert all(results[name]['peak'] > 100 for name in ['decision-main-voice', 'decision-main-voice-again']), 'Opening the main window did not play the question voice'
            current = json.loads(state_path.read_text())
            assert current['timer']['session']['id'] == 'decision-fixture' and current['timer']['session']['extensions'] == 1 and not current['sessions'], 'Question presentation changed session accounting'
        else:
            capture('menu-effects', lambda: [click(name) for name in ['Настройки', '♡ Комната фокуса'] * 4])
            time.sleep(.8)
            capture('single-high-first', lambda: click('Увеличить время работы на минуту'))
            capture('single-high-resumed', lambda: click('Увеличить время работы на минуту'))
            capture('single-low-resumed', lambda: click('Уменьшить время работы на минуту'))
            click('Переключить тихий режим')
            capture('quiet', lambda: click('Увеличить время работы на минуту'))
            click('Переключить тихий режим')
            capture('single-high-after-quiet', lambda: click('Увеличить время работы на минуту'))
            click('Помощник ↗'); click('Мику: клик — открыть, перетаскивание — переместить')
            capture('single-high-after-window-switch', lambda: click('Увеличить время работы на минуту'))
            click('Настройки'); click('Звуки'); time.sleep(.5)
            capture('html-audio-preview', lambda: click('Прослушать UI SFX_InGameMenu_Open.ogg'))
            time.sleep(2)  # Let the 2.6s preview end before tearing down its private output.
            capture('html-short-high-first', lambda: click('Прослушать high.wav'))
            capture('html-short-high-resumed', lambda: click('Прослушать high.wav'))
            capture('html-short-low', lambda: click('Прослушать low.wav'))
            assert results['quiet']['peak'] == 0, 'Quiet Mode leaked audio'
            assert all(result['peak'] > 100 and result['nonzero'] > 100 for name, result in results.items() if name != 'quiet'), 'No PCM output in one or more app playback paths'
            energies = [result['energy'] for name, result in results.items() if name.startswith('single-high')]
            assert max(energies) / min(energies) < 1.15, 'Identical short effects were truncated or changed level after idle/quiet/window switches'
            reference = results['html-short-high-first']['energy']
            assert all(.95 < energy / reference < 1.05 for energy in energies), 'Interface volume differs from preview at the same saved volume'
        passed = True
        print('Main-only decision voice passed' if args.decision else 'Both playback paths produced PCM; repeated short effects kept their level; Quiet Mode was silent', flush=True)
    finally:
        for process in [recorder, app]:
            if process is not None and process.poll() is None:
                process.terminate()
                try: process.wait(timeout=10)
                except subprocess.TimeoutExpired: process.kill(); process.wait()
        subprocess.run(['pactl', 'unload-module', module], check=True)
        assert subprocess.check_output(['pactl', 'get-default-sink'], text=True).strip() == default_sink, 'Default output unexpectedly changed'
        (base / 'summary.json').write_text(json.dumps({'passed': passed, 'mode': 'decision-panel' if args.panel else 'decision' if args.decision else 'effects', 'binary': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'pid': app.pid if app else None, 'defaultSinkUnchanged': True, 'privateModuleUnloaded': True}, indent=2))
