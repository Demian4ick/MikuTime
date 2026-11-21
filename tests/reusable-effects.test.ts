import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ReusableEffects } from '../src/lib/reusable-effects.ts';
import { SfxManager } from '../src/lib/sounds.ts';
import type { Settings } from '../src/lib/types.ts';
import type { AudioSource } from '../src/lib/voice.ts';

class FakeAudio {
  static instances: FakeAudio[] = [];
  private url = ''; srcChanges = 0; currentTime = 0; volume = 1; preload = ''; ended = false;
  onended: (() => void) | null = null; onerror: (() => void) | null = null;
  plays = 0; pauses = 0; loads = 0;
  constructor() { FakeAudio.instances.push(this); }
  get src() { return this.url; }
  set src(url: string) { this.url = url; this.srcChanges++; }
  play() { this.plays++; this.ended = false; return Promise.resolve(); }
  pause() { this.pauses++; }
  load() { this.loads++; }
  removeAttribute() { this.url = ''; }
  end() { this.ended = true; this.onended?.(); }
}
function mock() {
  const original = globalThis.Audio; FakeAudio.instances = [];
  Object.defineProperty(globalThis, 'Audio', { value: FakeAudio, configurable: true, writable: true });
  return () => {
    if (original === undefined) delete (globalThis as { Audio?: unknown }).Audio;
    else Object.defineProperty(globalThis, 'Audio', { value: original, configurable: true, writable: true });
  };
}
const turn = () => new Promise(resolve => setImmediate(resolve));
test('400 menu/hover events use only two reusable media elements and two lazy source loads', async () => {
  const restore = mock(), originalFetch = globalThis.fetch; let requests = 0;
  globalThis.fetch = async () => { requests++; return new Response(new Uint8Array([1, 2, 3])); };
  let settings = { quietMode: false, sfxVolume: 0.5, sfxBindings: { menu_choice: 'choice-hover', choice_hover: 'menu-hover' } } as unknown as Settings;
  const manager = new SfxManager(() => settings, p => p, assert.fail);
  try {
    assert.equal(FakeAudio.instances.length, 0);
    for (let i = 0; i < 200; i++) { void manager.event('choice_hover'); void manager.event('menu_choice'); await turn(); }
    assert.equal(requests, 2); assert.equal(FakeAudio.instances.length, 2);
    assert.ok(FakeAudio.instances.every(audio => audio.plays === 200 && audio.srcChanges === 1 && audio.loads === 0));
    settings = { ...settings, sfxVolume: 0.2 }; manager.configure(false);
    assert.ok(FakeAudio.instances.every(audio => audio.volume === 0.2));
    settings = { ...settings, quietMode: true }; manager.configure(false);
    assert.ok(FakeAudio.instances.every(audio => audio.loads === 1 && audio.src === ''));
    await manager.event('choice_hover'); assert.equal(FakeAudio.instances.length, 2);
  } finally { manager.stop(); globalThis.fetch = originalFetch; restore(); }
});
test('natural ending does not unload queued PCM; the next click rewinds the same loaded element', async () => {
  const restore = mock(); let loads = 0, releases = 0;
  const player = new ReusableEffects(() => { loads++; return { url: 'blob:high', release: () => releases++ }; }, assert.fail);
  try {
    const first = player.play('/sfx/high.wav', 0.5, 'main'); await turn();
    const audio = FakeAudio.instances[0]; audio.end(); await first;
    assert.equal(audio.loads, 0); assert.equal(audio.pauses, 0); assert.equal(releases, 0);
    audio.currentTime = 0.06;
    const second = player.play('/sfx/high.wav', 0.5, 'main'); await turn();
    assert.equal(audio.currentTime, 0); assert.equal(FakeAudio.instances.length, 1); assert.equal(audio.srcChanges, 1); assert.equal(loads, 1);
    audio.end(); await second;
    player.stop(); assert.equal(releases, 1); assert.equal(audio.loads, 1);
  } finally { player.stop(); restore(); }
});
test('pending restarts share one load, cancel immediately and release a late source after stop', async () => {
  const restore = mock(); let complete!: (source: AudioSource) => void; let loads = 0, releases = 0;
  const player = new ReusableEffects(() => { loads++; return new Promise<AudioSource>(resolve => complete = resolve); }, assert.fail);
  try {
    const old = player.play('/sfx/high.wav', 0.5, 'main'); await turn();
    const next = player.play('/sfx/high.wav', 0.5, 'main'); await old;
    assert.equal(loads, 1);
    player.stop(); await next;
    complete({ url: 'blob:late', release: () => releases++ }); await turn();
    assert.equal(releases, 1); assert.equal(FakeAudio.instances.length, 0);
  } finally { player.stop(); restore(); }
});
test('switching files reuses the same channel and returns to its cached source', async () => {
  const restore = mock(); let loads = 0, releases = 0;
  const player = new ReusableEffects(path => { loads++; return { url: path, release: () => releases++ }; }, assert.fail);
  try {
    const first = player.play('/sfx/high.wav', 0.5, 'main'); await turn();
    const second = player.play('/sfx/low.wav', 0.5, 'main'); await first; await turn();
    const third = player.play('/sfx/high.wav', 0.5, 'main'); await second; await turn();
    assert.equal(loads, 2); assert.equal(FakeAudio.instances.length, 1); assert.equal(FakeAudio.instances[0].srcChanges, 3);
    player.stop(); await third; assert.equal(releases, 2);
  } finally { player.stop(); restore(); }
});
test('shared source is released once across both channels and live/pending volumes are respected', async () => {
  const restore = mock(); let releases = 0;
  const player = new ReusableEffects(() => ({ url: 'blob:shared', release: () => releases++ }), assert.fail);
  try {
    const main = player.play('/sfx/high.wav', 0.5, 'main'); const hover = player.play('/sfx/high.wav', 0.5, 'hover');
    player.configure(0.2); await turn(); assert.ok(FakeAudio.instances.every(audio => audio.volume === 0.2));
    player.stopChannel('hover'); await hover; assert.equal(releases, 0);
    player.configure(0.1); assert.ok(FakeAudio.instances.every(audio => audio.volume === 0.1));
    player.stop(); await main; assert.equal(releases, 1);
  } finally { player.stop(); restore(); }
});
test('failed source loads are reported and evicted so the next click can retry', async () => {
  const restore = mock(); let loads = 0; const errors: string[] = [];
  const player = new ReusableEffects(() => { if (++loads === 1) throw new Error('test'); return '/sfx/high.wav'; }, error => errors.push(error));
  try {
    await player.play('/sfx/high.wav', 0.5, 'main'); assert.equal(errors.length, 1);
    const next = player.play('/sfx/high.wav', 0.5, 'main'); await turn();
    assert.equal(loads, 2); FakeAudio.instances[0].end(); await next;
  } finally { player.stop(); restore(); }
});
