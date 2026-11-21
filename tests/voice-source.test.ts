import { test } from 'node:test';
import assert from 'node:assert/strict';
import { loadVoiceSource, VoiceManager } from '../src/lib/voice.ts';
import type { Settings } from '../src/lib/types.ts';

test('local voice fetch is lazy and normalizes WAV/OGG MIME before releasing the blob URL', async () => {
  const originalFetch = globalThis.fetch; let requests = 0;
  globalThis.fetch = async input => { requests++; assert.match(String(input), /^asset:/); return new Response(new Uint8Array([1, 2, 3]), { headers: { 'content-type': 'audio/wave' } }); };
  try {
    assert.equal(requests, 0);
    for (const [file, type] of [['question.WAV', 'audio/wav'], ['question.ogg', 'audio/ogg']]) {
      const source = await loadVoiceSource(file, path => `asset:${path}`);
      assert.equal((await originalFetch(source.url)).headers.get('content-type'), type);
      source.release(); await assert.rejects(originalFetch(source.url));
    }
    assert.equal(requests, 2);
  } finally { globalThis.fetch = originalFetch; }
});
test('voice source errors and files above the import limit are rejected instead of played', async () => {
  const originalFetch = globalThis.fetch;
  try {
    globalThis.fetch = async () => new Response('', { status: 403 });
    await assert.rejects(loadVoiceSource('question.wav', path => path), /открыть локальную озвучку/);
    globalThis.fetch = async () => new Response('', { headers: { 'content-length': String(20 * 1024 * 1024 + 1) } });
    await assert.rejects(loadVoiceSource('question.wav', path => path), /20 МиБ/);
  } finally { globalThis.fetch = originalFetch; }
});
test('hiding during a pending local voice fetch prevents late audio and revokes its source', async () => {
  const originalFetch = globalThis.fetch, originalAudio = globalThis.Audio, create = URL.createObjectURL;
  let respond!: (response: Response) => void, url = '', signal: AbortSignal | undefined;
  globalThis.fetch = async (_, init) => { signal = init?.signal ?? undefined; return new Promise(resolve => respond = resolve); };
  URL.createObjectURL = blob => { url = create(blob); return url; };
  Object.defineProperty(globalThis, 'Audio', { value: class { constructor() { assert.fail('hidden voice must not create a player'); } }, configurable: true, writable: true });
  const player = new VoiceManager();
  try {
    const playing = player.play('question', { quietMode: false, volume: 0.5, voices: { question: '/local/question.wav' } } as Settings, (path, signal) => loadVoiceSource(path, path => `asset:${path}`, signal), new AbortController().signal);
    player.stop(); assert.equal(signal?.aborted, true); respond(new Response(new Uint8Array([1, 2, 3]))); await playing;
    assert.match(url, /^blob:/); await assert.rejects(originalFetch(url));
  } finally {
    player.stop(); globalThis.fetch = originalFetch; URL.createObjectURL = create;
    if (originalAudio === undefined) delete (globalThis as { Audio?: unknown }).Audio;
    else Object.defineProperty(globalThis, 'Audio', { value: originalAudio, configurable: true, writable: true });
  }
});
