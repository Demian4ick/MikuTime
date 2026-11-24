import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ArrowStepInput, hoverButton, closePresentation, exitFadeMs, exitAudioLimitMs, exitFarewellMs, exitVoiceLimitMs, exitFarewell, farewellLine, farewellPresentation } from '../src/lib/interaction.ts';
import { defaultSoundBindings } from '../src/lib/sounds.ts';
import { readFileSync } from 'node:fs';

test('approved bindings match the requested buttons, hover and shutdown', () => {
  assert.equal(Object.keys(defaultSoundBindings).length, 20);
  for (const [event, sound] of Object.entries({ app_quit: 'game-menu-close', duration_up: 'high', duration_down: 'low', menu_open: 'menu-open', choice_hover: 'menu-hover', menu_choice: 'choice-hover' })) assert.equal(defaultSoundBindings[event as keyof typeof defaultSoundBindings], sound);
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
  await assert.rejects(closePresentation(async () => { throw new Error('save failed'); }, unexpected, unexpected, unexpected, unexpected), /save failed/);
});
test('farewell, audio and fading start together after saving; exit still waits for the voice', async () => {
  const events: string[] = []; let endFarewell!: () => void;
  const closing = closePresentation(
    async () => { events.push('flush'); },
    async () => { events.push('audio'); },
    async () => { events.push('fade'); },
    async () => { events.push('exit'); },
    () => { events.push('farewell'); return new Promise(resolve => endFarewell = resolve); },
  );
  await Promise.resolve(); assert.deepEqual(events, ['flush', 'farewell', 'audio', 'fade']);
  await Promise.resolve(); assert.ok(!events.includes('exit'));
  endFarewell(); await closing;
  assert.deepEqual(events, ['flush', 'farewell', 'audio', 'fade', 'exit']);
  assert.equal(exitFarewell, 'Я буду ждать нашей следующей встречи ♡');
  assert.equal(exitFarewellMs, 1500);
  assert.equal(Math.max(exitFarewellMs, exitVoiceLimitMs, exitFadeMs, exitAudioLimitMs), 3500);
});
test('a failed farewell cancels exit even though the concurrent fade and sound have started', async () => {
  const events: string[] = [];
  await assert.rejects(closePresentation(async () => {}, async () => { events.push('audio'); }, async () => { events.push('fade'); }, async () => assert.fail('must not exit'), async () => { throw new Error('cancelled'); }), /cancelled/);
  assert.deepEqual(events, ['audio', 'fade']);
});
test('a short farewell cannot close the process before the concurrent fade and sound finish', async () => {
  let endAudio!: () => void, endFade!: () => void, exited = false;
  const closing = closePresentation(async () => {}, () => new Promise(resolve => endAudio = resolve), () => new Promise(resolve => endFade = resolve), async () => { exited = true; }, async () => {});
  await Promise.resolve(); await Promise.resolve(); assert.equal(exited, false);
  endAudio(); await Promise.resolve(); assert.equal(exited, false);
  endFade(); await closing; assert.equal(exited, true);
});
test('farewell reuses the ordinary dialogue and companion bubble, never a separate card', () => {
  const app = readFileSync(new URL('../src/App.svelte', import.meta.url), 'utf8');
  const css = readFileSync(new URL('../src/style.css', import.meta.url), 'utf8');
  const panel = readFileSync(new URL('../src/components/DialoguePanel.svelte', import.meta.url), 'utf8');
  assert.ok(app.includes("if (!companion) page = 'focus'"));
  assert.ok(app.includes('ru: line.ru'));
  assert.ok(app.includes('<DialoguePanel {visual} passive={closing}'));
  assert.ok(app.includes('id="companion-dialogue-text"'));
  assert.ok(panel.includes('{#if !passive}'));
  assert.ok(!app.includes('exit-farewell') && !css.includes('.exit-farewell'));
});
test('farewell uses the bundled line or a custom exit line without mutating the catalogue', () => {
  const data = JSON.parse(readFileSync(new URL('../public/dialogues.json', import.meta.url), 'utf8'));
  assert.deepEqual(farewellLine(null), farewellLine(data));
  const line = { en: 'Custom', ru: 'До встречи', emotion: 'Happy' as const, voice: 'custom_exit' };
  const custom = { version: 1, dialogues: [{ id: 'custom', event: 'app_quit', priority: 1, cooldownMs: 0, lines: [line] }] };
  assert.equal(farewellLine(custom), line);
  assert.deepEqual(custom.dialogues[0].lines, [line]);
});
test('farewell waits for natural voice completion before allowing process exit', async () => {
  let ended!: () => void, stopped = false, done = false;
  const playing = farewellPresentation(() => new Promise(resolve => ended = resolve), () => stopped = true, 0, 1000).then(() => done = true);
  await new Promise(resolve => setTimeout(resolve, 5)); assert.equal(done, false);
  ended(); await playing; assert.equal(done, true); assert.equal(stopped, true);
});
test('missing farewell voice retains the minimum reading interval and does not hold exit open', async () => {
  let stopped = false;
  await farewellPresentation(async () => {}, () => stopped = true, 5, 1000);
  assert.equal(stopped, true);
});
test('stalled farewell loading is bounded and stopped even if its promise never resolves', async () => {
  let stopped = false;
  await farewellPresentation(() => new Promise(() => {}), () => stopped = true, 0, 5);
  assert.equal(stopped, true);
});
test('a rejected farewell voice releases its player and both one-shot timers', async () => {
  let stopped = false;
  await assert.rejects(farewellPresentation(async () => { throw new Error('voice failed'); }, () => stopped = true, 1000, 1000), /voice failed/);
  assert.equal(stopped, true);
});
