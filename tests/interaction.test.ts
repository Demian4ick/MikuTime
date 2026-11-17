import { test } from 'node:test';
import assert from 'node:assert/strict';
import { hoverButton, closePresentation, exitFadeMs, exitAudioLimitMs } from '../src/lib/interaction.ts';
import { defaultSoundBindings } from '../src/lib/sounds.ts';

test('approved bindings match the requested buttons, hover and shutdown', () => {
  assert.deepEqual(defaultSoundBindings, { app_quit: 'game-menu-close', duration_up: 'high', duration_down: 'low', menu_open: 'menu-open', choice_hover: 'menu-hover' });
  assert.ok(exitFadeMs < 2523 && exitAudioLimitMs > 2523);
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
