import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ArrowStepInput, hoverButton, closePresentation, exitFadeMs, exitAudioLimitMs } from '../src/lib/interaction.ts';
import { defaultSoundBindings } from '../src/lib/sounds.ts';

test('approved bindings match the requested buttons, hover and shutdown', () => {
  assert.deepEqual(defaultSoundBindings, { app_quit: 'game-menu-close', duration_up: 'high', duration_down: 'low', menu_open: 'menu-open', choice_hover: 'menu-hover', menu_choice: 'choice-hover' });
  assert.ok(exitFadeMs < 2523 && exitAudioLimitMs > 2523);
});
test('numeric keyboard arrows report the actual change direction', () => {
  const input = new ArrowStepInput();
  input.keyDown('ArrowUp', 'number'); assert.equal(input.direction(10, 11), 1);
  input.keyDown('ArrowDown', 'number'); assert.equal(input.direction(11, 10), -1);
  input.keyUp(); assert.equal(input.direction(10, 11), null);
});
test('native numeric spinners repeat while held, but boundaries and invalid drafts stay silent', () => {
  const input = new ArrowStepInput(); input.pointerDown(0, 'number');
  assert.equal(input.direction(10, 11), 1); assert.equal(input.direction(11, 12), 1);
  assert.equal(input.direction(12, 11), -1); assert.equal(input.direction(480, 480), null);
  assert.equal(input.direction(undefined, 1), null); assert.equal(input.direction(1, undefined), null);
  assert.equal(input.direction(NaN, 1), null); assert.equal(input.direction(1, Infinity), null);
  input.pointerEnd(); assert.equal(input.direction(11, 12), null);
});
test('typing, paste, caret navigation and external changes are not mistaken for arrow steps', () => {
  const input = new ArrowStepInput();
  assert.equal(input.direction(10, 11), null);
  input.pointerDown(0, 'number'); input.keyDown('1', 'number'); assert.equal(input.direction(10, 11), null);
  input.keyDown('v', 'number'); assert.equal(input.direction(11, 22), null);
  input.keyDown('ArrowLeft', 'number'); assert.equal(input.direction(22, 21), null);
  input.pointerDown(2, 'number'); assert.equal(input.direction(21, 22), null);
});
test('range arrow keys support decimal steps, while dragging and Home/End stay silent', () => {
  const input = new ArrowStepInput();
  input.pointerDown(0, 'range'); assert.equal(input.direction(0.5, 0.6), null);
  input.keyDown('ArrowRight', 'range'); assert.equal(input.direction(0.5, 0.55), 1);
  input.keyDown('ArrowLeft', 'range'); assert.equal(input.direction(0.55, 0.5), -1);
  input.keyDown('Home', 'range'); assert.equal(input.direction(0.5, 0), null);
  input.keyDown('End', 'range'); assert.equal(input.direction(0, 1), null);
});
test('blur and cancelled input clear arrow intent without any timers', () => {
  const input = new ArrowStepInput(); input.keyDown('ArrowUp', 'number'); input.reset();
  assert.equal(input.direction(10, 11), null);
  input.pointerDown(0, 'number'); input.reset(); assert.equal(input.direction(10, 11), null);
});
test('hover only targets enabled buttons and excludes characters, dialogue and preview areas', () => {
  const child = {} as Node;
  let excluded = false, disabled = false, inside = false;
  const button = {
    closest: (selector: string) => selector === '[data-hover-exclude], [data-no-sfx]' && excluded ? {} : null,
    matches: () => disabled, contains: () => inside,
  } as unknown as Element;
  const target = { closest: (selector: string) => selector === 'button' ? button : null } as unknown as Element;
  assert.equal(hoverButton(target, null), button);
  inside = true; assert.equal(hoverButton(target, child), null); inside = false;
  disabled = true; assert.equal(hoverButton(target, null), null); disabled = false;
  excluded = true; assert.equal(hoverButton(target, null), null);
  const select = { closest: () => null } as unknown as Element;
  assert.equal(hoverButton(select, null), null);
});
test('shutdown flushes first and waits for both black fade and the end of the close sound', async () => {
  const events: string[] = []; let endAudio!: () => void, endFade!: () => void;
  const closing = closePresentation(
    async () => { events.push('flush'); },
    () => { events.push('audio'); return new Promise(resolve => endAudio = resolve); },
    () => { events.push('fade'); return new Promise(resolve => endFade = resolve); },
    async () => { events.push('exit'); },
  );
  await Promise.resolve(); assert.deepEqual(events, ['flush', 'audio', 'fade']);
  endFade(); await Promise.resolve(); assert.ok(!events.includes('exit'));
  endAudio(); await closing; assert.equal(events.at(-1), 'exit');
});
test('a failed settings flush prevents both presentation and exit', async () => {
  const unexpected = async () => assert.fail('must not run');
  await assert.rejects(closePresentation(async () => { throw new Error('save failed'); }, unexpected, unexpected, unexpected), /save failed/);
});
