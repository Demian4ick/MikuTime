import { test } from 'node:test';
import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';
import { ReactionController, wait } from '../src/lib/reactions.ts';
import { VoiceManager } from '../src/lib/voice.ts';
import type { DialogueFile, Settings } from '../src/lib/types.ts';

const settings = { quietMode: true, voices: {}, volume: 0.5 } as Settings;
const data: DialogueFile = { version: 1, dialogues: [
  { id: 'low', event: 'low', priority: 1, cooldownMs: 1000, lines: [{ en: 'low', ru: '', emotion: 'Calm', durationMs: 100 }] },
  { id: 'high', event: 'high', priority: 10, cooldownMs: 0, lines: [{ en: 'high', ru: '', emotion: 'Happy', durationMs: 100 }] },
] };
test('higher-priority reaction interrupts and cooldown prevents repeats', async () => {
  const lines: string[] = [];
  const controller = new ReactionController(data, () => settings, p => p, v => { if (v.speaking) lines.push(v.en); });
  controller.event('low'); controller.event('low');
  assert.deepEqual(lines, ['low']);
  controller.event('high'); await delay(10);
  assert.deepEqual(lines, ['low', 'high']);
  controller.dispose(); await delay(10); controller.event('high');
  assert.deepEqual(lines, ['low', 'high']);
});
test('abort clears wait and dispose cancels queued emotions', async () => {
  const abort = new AbortController(); const pending = wait(10000, abort.signal); abort.abort(); await pending;
  const emotions: string[] = [];
  const controller = new ReactionController({ version: 1, dialogues: [{ id: 'seq', event: 'seq', priority: 1, cooldownMs: 0, lines: [
    { en: 'one', ru: '', emotion: 'Surprised', durationMs: 100 }, { en: 'two', ru: '', emotion: 'Pleased', durationMs: 100 },
  ] }] }, () => settings, p => p, v => emotions.push(v.emotion));
  controller.event('seq'); controller.dispose(); await delay(10);
  assert.deepEqual(emotions, ['Surprised']);
});
test('missing voices and Quiet Mode work without any Audio object', async () => {
  const manager = new VoiceManager(); const signal = new AbortController().signal;
  await manager.play('missing', settings, p => p, signal);
  await manager.play('missing', { ...settings, quietMode: false }, p => p, signal);
  manager.stop();
});
test('rest context interrupts work, rejects resume, and returns to rest-specific text', async () => {
  const states: { en: string; ru: string; speaking: boolean }[] = [];
  const file: DialogueFile = { version: 1, dialogues: [
    { id: 'resume', event: 'resume', priority: 3, cooldownMs: 0, lines: [{ en: 'work-resume', ru: '', emotion: 'Happy', durationMs: 1000 }] },
    { id: 'rest', event: 'break_start', priority: 12, cooldownMs: 0, lines: [{ en: 'rest-now', ru: '', emotion: 'Calm', durationMs: 100 }] },
  ] };
  const controller = new ReactionController(file, () => settings, p => p, v => states.push(v));
  controller.event('resume');
  controller.setContext(true); controller.event('resume'); controller.event('break_start');
  await delay(160);
  assert.deepEqual(states.filter(v => v.speaking).map(v => v.en), ['work-resume', 'rest-now']);
  assert.match(states.at(-1)!.ru, /Отдыхай/);
  controller.event('resume'); assert.equal(states.filter(v => v.speaking).length, 2);
  controller.setContext(false); assert.match(states.at(-1)!.ru, /Всё получится/);
  controller.dispose();
});
test('voice is lazy, uses the imported path, updates volume, and releases on stop', async () => {
  class FakeAudio {
    static instances: FakeAudio[] = [];
    src = ''; volume = 1; preload = ''; onended: (() => void) | null = null; onerror: (() => void) | null = null;
    played = false; paused = false; released = false;
    constructor() { FakeAudio.instances.push(this); }
    play() { this.played = true; return Promise.resolve(); }
    pause() { this.paused = true; }
    removeAttribute(name: string) { if (name === 'src') this.src = ''; }
    load() { this.released = true; }
  }
  const original = globalThis.Audio;
  Object.defineProperty(globalThis, 'Audio', { value: FakeAudio, configurable: true, writable: true });
  try {
    const manager = new VoiceManager(); const current = { ...settings, quietMode: false, voices: { test: '/local/test.wav' } };
    const playing = manager.play('test', current, p => `asset:${p}`, new AbortController().signal);
    const audio = FakeAudio.instances[0]; assert.equal(audio.src, 'asset:/local/test.wav'); assert.equal(audio.played, true);
    manager.configure({ ...current, volume: 0.2 }); assert.equal(audio.volume, 0.2);
    manager.stop(); await playing; assert.equal(audio.released, true); assert.equal(audio.paused, true);
  } finally { if (original === undefined) delete (globalThis as { Audio?: unknown }).Audio; else Object.defineProperty(globalThis, 'Audio', { value: original, configurable: true, writable: true }); }
});
test('instant text reveals complete Unicode characters and live speed changes finish typing', async () => {
  const lines: string[] = [];
  const current = { ...settings, textSpeed: 1 };
  const controller = new ReactionController({ version: 1, dialogues: [{ id: 'typing', event: 'touch', priority: 1, cooldownMs: 0,
    lines: [{ en: '', ru: 'Мику 🎵 рядом', emotion: 'Calm', durationMs: 0 }] }] }, () => current, p => p, value => { if (value.speaking) lines.push(value.ru); });
  controller.event('touch'); await delay(50);
  assert.ok(!lines.includes('Мику 🎵 рядом'));
  current.textSpeed = 0; await delay(70);
  assert.ok(lines.includes('Мику 🎵 рядом'));
  for (const line of lines) assert.ok(!/[\uD800-\uDBFF]$/.test(line));
  controller.dispose();
});
