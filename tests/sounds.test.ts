import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { builtinEffects, effectRows, effectPath, fileName, soundEvents, soundUrl, loadSoundSource, voiceRows, SfxManager, SoundPreview } from '../src/lib/sounds.ts';
import { VoiceManager, type AudioSource } from '../src/lib/voice.ts';
import type { DialogueFile, Settings } from '../src/lib/types.ts';

const defaults = { voices: {}, sfx: {}, sfxBindings: {}, volume: 0.7, sfxVolume: 0.5, quietMode: false } as Settings;
class FakeAudio {
  static instances: FakeAudio[] = [];
  src = ''; volume = 1; preload = ''; onended: (() => void) | null = null; onerror: (() => void) | null = null;
  released = false; paused = false;
  constructor() { FakeAudio.instances.push(this); }
  play() { return Promise.resolve(); }
  pause() { this.paused = true; }
  removeAttribute() { this.src = ''; }
  load() { this.released = true; }
}
function mockAudio() {
  const original = globalThis.Audio; FakeAudio.instances = [];
  Object.defineProperty(globalThis, 'Audio', { value: FakeAudio, configurable: true, writable: true });
  return () => {
    if (original === undefined) delete (globalThis as { Audio?: unknown }).Audio;
    else Object.defineProperty(globalThis, 'Audio', { value: original, configurable: true, writable: true });
  };
}
const url = (path: string) => `asset:${path}`;

test('catalogue matches all nine user originals byte-for-byte; event IDs are unique', () => {
  assert.equal(builtinEffects.length, 9);
  assert.equal(new Set(builtinEffects.map(e => e.id)).size, 9);
  assert.equal(new Set(soundEvents.map(e => e.id)).size, soundEvents.length);
  for (const effect of builtinEffects) {
    assert.match(effect.name, /[А-Яа-яЁё]/);
    assert.match(effect.file, /^\/sfx\/[a-z-]+\.(wav|ogg)$/);
    assert.deepEqual(readFileSync(new URL(`../public${effect.file}`, import.meta.url)), readFileSync(new URL(`../sfx/${effect.originalFile}`, import.meta.url)));
  }
  for (const event of soundEvents) assert.match(event.name, /[А-Яа-яЁё]/);
});
test('effect rows preserve bundled fallback, overrides, and imported effects without mutations', () => {
  const settings = { ...defaults, sfx: { high: '/local/new.wav', custom: 'C:\\Music\\custom.ogg' } };
  const rows = effectRows(settings);
  assert.equal(rows.length, 10);
  assert.equal(rows.find(r => r.id === 'high')!.path, '/local/new.wav');
  assert.equal(rows.find(r => r.id === 'custom')!.builtin, false);
  assert.equal(effectPath(settings, 'low'), '/sfx/low.wav');
  assert.equal(effectPath(settings, 'missing'), undefined);
  assert.equal(fileName('C:\\Music\\custom.ogg'), 'custom.ogg');
  assert.equal(soundUrl('/sfx/high.wav', url), '/sfx/high.wav');
  assert.equal(soundUrl('/local/new.wav', url), 'asset:/local/new.wav');
  assert.deepEqual(settings.sfx, { high: '/local/new.wav', custom: 'C:\\Music\\custom.ogg' });
});
test('voice table includes missing voices, repeated references, and unused imported files', () => {
  const data: DialogueFile = { version: 1, dialogues: [{ id: 'hello', event: 'start', priority: 1, cooldownMs: 0, lines: [
    { ru: 'Привет!', en: 'Do not display this', emotion: 'Happy', voice: 'hello' },
    { ru: 'Ещё раз', en: '', emotion: 'Calm', voice: 'hello' },
    { ru: 'Тихо', en: '', emotion: 'Calm', voice: 'missing' },
  ] }] };
  const rows = voiceRows(data, { ...defaults, voices: { hello: '/hello.wav', unused: '/unused.ogg' } });
  assert.equal(rows.length, 3);
  assert.equal(rows[0].uses, 2); assert.equal(rows[0].text, 'Привет!');
  assert.equal(rows[0].event, 'Начало работы'); assert.equal(rows[1].path, undefined);
  assert.equal(rows[2].event, 'Не используется в диалогах');
  assert.equal(voiceRows(null, defaults).length, 0);
});
test('empty bindings, unknown events, missing custom files and Quiet Mode allocate no audio', () => {
  const restore = mockAudio(); let settings = defaults;
  const manager = new SfxManager(() => settings, url, assert.fail, soundUrl);
  try {
    manager.configure(false); manager.event('start'); manager.event('unknown');
    settings = { ...defaults, quietMode: true, sfxBindings: { start: 'high' } }; manager.event('start');
    settings = { ...defaults, sfxBindings: { start: 'missing' } }; manager.event('start');
    assert.equal(FakeAudio.instances.length, 0);
  } finally { manager.stop(); restore(); }
});
test('hover restarts immediately on its own channel without cutting off timer signals', () => {
  const restore = mockAudio();
  const originalNow = Date.now; Date.now = () => 1000;
  const settings = { ...defaults, sfxBindings: { start: 'high', choice_hover: 'low', pause: 'soft' } };
  const manager = new SfxManager(() => settings, url, assert.fail, soundUrl);
  try {
    manager.event('start'); const first = FakeAudio.instances[0];
    assert.equal(first.src, '/sfx/high.wav'); assert.equal(first.volume, 0.5); assert.equal(first.preload, 'none');
    manager.event('start'); assert.equal(FakeAudio.instances.length, 1);
    manager.event('choice_hover'); assert.equal(FakeAudio.instances.length, 2);
    assert.equal(first.released, false);
    Date.now = () => 900; // Zero-cooldown feedback must not wait for a wall-clock rollback either.
    manager.event('choice_hover'); assert.equal(FakeAudio.instances.length, 3);
    assert.equal(FakeAudio.instances[1].released, true); assert.equal(first.released, false);
    manager.event('pause'); assert.equal(FakeAudio.instances.length, 4); assert.equal(first.released, true);
    assert.equal(FakeAudio.instances[2].released, false);
    manager.stop(); assert.ok(FakeAudio.instances.every(audio => audio.released));
  } finally { manager.stop(); Date.now = originalNow; restore(); }
});
test('rest context stops work SFX, rejects work events and allows only rest or general effects', () => {
  const restore = mockAudio();
  const settings = { ...defaults, sfxBindings: { resume: 'high', break_start: 'soft', menu_open: 'menu-open' } };
  const manager = new SfxManager(() => settings, url, assert.fail, soundUrl);
  try {
    manager.event('resume'); manager.configure(true); assert.equal(FakeAudio.instances[0].released, true);
    manager.event('resume'); assert.equal(FakeAudio.instances.length, 1);
    manager.event('break_start'); assert.equal(FakeAudio.instances[1].src, '/sfx/soft.wav');
    manager.event('menu_open'); assert.equal(FakeAudio.instances[2].src, '/sfx/menu-open.wav');
    manager.configure(false); manager.event('break_start'); assert.equal(FakeAudio.instances.length, 3);
  } finally { manager.stop(); restore(); }
});
test('SFX volume and Quiet Mode apply to both channels without changing voice volume', () => {
  const restore = mockAudio(); let settings = { ...defaults, sfxBindings: { start: 'high', choice_hover: 'menu-hover' } };
  const manager = new SfxManager(() => settings, url, assert.fail, soundUrl);
  try {
    manager.event('start'); manager.event('choice_hover');
    settings = { ...settings, sfxVolume: 0.2 }; manager.configure(false);
    assert.ok(FakeAudio.instances.every(audio => audio.volume === 0.2)); assert.equal(settings.volume, 0.7);
    settings = { ...settings, quietMode: true }; manager.configure(false);
    assert.ok(FakeAudio.instances.every(audio => audio.released));
  } finally { manager.stop(); restore(); }
});
test('inner menu clicks use choice-hover and can repeat while hover audio is still active', () => {
  const restore = mockAudio();
  const settings = { ...defaults, sfxBindings: { menu_choice: 'choice-hover', choice_hover: 'menu-hover' } };
  const manager = new SfxManager(() => settings, url, assert.fail, soundUrl);
  try {
    manager.event('choice_hover'); const hover = FakeAudio.instances[0];
    manager.event('menu_choice'); assert.equal(FakeAudio.instances[1].src, '/sfx/choice-hover.wav');
    manager.event('menu_choice'); assert.equal(FakeAudio.instances[2].src, '/sfx/choice-hover.wav');
    assert.equal(FakeAudio.instances[1].released, true); assert.equal(hover.released, false);
    manager.event('choice_hover'); assert.equal(FakeAudio.instances[3].src, '/sfx/menu-hover.ogg');
    assert.equal(hover.released, true); assert.equal(FakeAudio.instances[2].released, false);
  } finally { manager.stop(); restore(); }
});
test('stopping cancels both pending hover loads without later playback or leaking their sources', async () => {
  const restore = mockAudio(); const loads: ((source: AudioSource) => void)[] = []; let releases = 0;
  const manager = new SfxManager(() => ({ ...defaults, sfxBindings: { start: 'high', choice_hover: 'menu-hover' } }), url, assert.fail,
    path => path === '/sfx/menu-hover.ogg' ? new Promise<AudioSource>(resolve => loads.push(resolve)) : path);
  try {
    const main = manager.event('start'); const oldHover = manager.event('choice_hover'); const newHover = manager.event('choice_hover');
    assert.equal(loads.length, 2); assert.equal(FakeAudio.instances[0].released, false);
    manager.stop();
    loads[1]({ url: 'blob:new', release: () => releases++ }); loads[0]({ url: 'blob:old', release: () => releases++ });
    await Promise.all([main, oldHover, newHover]);
    assert.equal(releases, 2); assert.equal(FakeAudio.instances.length, 1); assert.equal(FakeAudio.instances[0].released, true);
  } finally { manager.stop(); restore(); }
});
test('preview is lazy, switches sound types, supports zero volume, and stop releases playback', async () => {
  const restore = mockAudio(); const preview = new SoundPreview(assert.fail, soundUrl);
  try {
    assert.equal(FakeAudio.instances.length, 0);
    const first = preview.play('sfx', 'game-menu-open', { ...defaults, sfxVolume: 0 }, url);
    assert.equal(FakeAudio.instances[0].volume, 0); assert.equal(FakeAudio.instances[0].src, '/sfx/game-menu-open.ogg');
    const second = preview.play('voice', 'hello', { ...defaults, voices: { hello: '/local/hello.wav' } }, url);
    await first; assert.equal(FakeAudio.instances[0].released, true);
    assert.equal(FakeAudio.instances[1].src, 'asset:/local/hello.wav');
    preview.configure({ ...defaults, volume: 0.3 }); assert.equal(FakeAudio.instances[1].volume, 0.3);
    preview.stop(); await second; assert.equal(FakeAudio.instances[1].released, true);
    await preview.play('sfx', 'high', { ...defaults, quietMode: true }, url);
    assert.equal(FakeAudio.instances.length, 2);
  } finally { preview.stop(); restore(); }
});
test('preview naturally ending or Quiet Mode resolves the table wait and frees the player', async () => {
  const restore = mockAudio(); const preview = new SoundPreview(assert.fail, soundUrl);
  try {
    const first = preview.play('sfx', 'high', defaults, url); FakeAudio.instances[0].onended!(); await first;
    const second = preview.play('sfx', 'low', defaults, url); preview.configure({ ...defaults, quietMode: true }); await second;
    assert.ok(FakeAudio.instances.every(a => a.released && a.paused));
  } finally { preview.stop(); restore(); }
});
test('bundled audio is fetched lazily into a typed blob and revoked after playback', async () => {
  const restore = mockAudio(); const originalFetch = globalThis.fetch;
  let requests = 0;
  globalThis.fetch = async input => { requests++; assert.equal(input, '/sfx/high.wav'); return new Response(new Uint8Array([1, 2, 3])); };
  const preview = new SoundPreview(assert.fail);
  try {
    assert.equal(loadSoundSource('/local/voice.wav', url), 'asset:/local/voice.wav');
    assert.equal(requests, 0);
    const playing = preview.play('sfx', 'high', defaults, url);
    // Let fetch/arrayBuffer resolve; no audio is created while reading the source.
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(requests, 1);
    const blobUrl = FakeAudio.instances[0].src;
    assert.match(blobUrl, /^blob:/);
    assert.equal((await originalFetch(blobUrl)).headers.get('content-type'), 'audio/wav');
    preview.stop(); await playing;
    await assert.rejects(originalFetch(blobUrl));
  } finally { preview.stop(); globalThis.fetch = originalFetch; restore(); }
});
test('stopping during asynchronous source loading prevents late playback and releases the source', async () => {
  const restore = mockAudio(); const player = new VoiceManager();
  let releaseSource: ((source: { url: string; release: () => void }) => void) | undefined;
  let released = false;
  try {
    const playing = player.play('hello', { ...defaults, voices: { hello: '/hello.wav' } }, () => new Promise(resolve => releaseSource = resolve), new AbortController().signal);
    player.stop(); releaseSource!({ url: 'blob:late', release: () => released = true }); await playing;
    assert.equal(released, true); assert.equal(FakeAudio.instances.length, 0);
  } finally { player.stop(); restore(); }
});
