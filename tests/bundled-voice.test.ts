import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { builtinVoices, builtinVoicePath, loadVoiceSource, VoiceManager } from '../src/lib/voice.ts';
import { defaultSoundBindings, voiceRows } from '../src/lib/sounds.ts';
import type { DialogueFile, Settings } from '../src/lib/types.ts';

const voices = Object.fromEntries(builtinVoices.map(voice => [voice.id, voice.file]));
test('all 21 supplied WAV recordings are shipped intact, uniquely bound, with no extra assets', () => {
  const dialogues = JSON.parse(readFileSync(new URL('../public/dialogues.json', import.meta.url), 'utf8')) as DialogueFile;
  const ids = new Set(dialogues.dialogues.flatMap(dialogue => dialogue.lines.flatMap(line => line.voice ? [line.voice] : [])));
  assert.equal(builtinVoices.length, 21); assert.equal(new Set(builtinVoices.map(voice => voice.id)).size, 21);
  assert.deepEqual(new Set(Object.keys(voices)), ids);
  assert.deepEqual(readdirSync(new URL('../public/voices/', import.meta.url)).sort(), builtinVoices.map(voice => `${voice.id}.wav`).sort());
  let total = 0;
  for (const voice of builtinVoices) {
    assert.match(voice.file, /^\/voices\/[a-z_]+\.wav$/);
    const bytes = readFileSync(new URL(`../public${voice.file}`, import.meta.url));
    assert.equal(bytes.length, voice.bytes); assert.equal(bytes.toString('ascii', 0, 4), 'RIFF'); assert.equal(bytes.toString('ascii', 8, 12), 'WAVE');
    assert.equal(createHash('sha256').update(bytes).digest('hex'), voice.sha256);
    assert.ok(voice.durationMs > 0 && bytes.length < 20 * 1024 * 1024); total += bytes.length;
  }
  assert.equal(total, 7866654);
});
test('fresh sound bindings match the supplied laptop configuration without private paths', () => {
  assert.deepEqual(defaultSoundBindings, {
    app_quit: 'game-menu-close', break_skipped: 'game-menu-close', break_start: 'choice-hover', choice_hover: 'menu-hover',
    dialogue_continue: 'soft', duration_down: 'low', duration_up: 'high', extension: 'choice-hover', first_extension: 'choice-hover',
    focus_complete: 'game-menu-open', goal: 'high', launch: 'game-menu-open', main_open: 'menu-open', menu_choice: 'choice-hover',
    menu_close: 'low', menu_open: 'menu-open', pause: 'low', reset: 'menu-close', resume: 'menu-close', start: 'game-menu-close',
  });
});
test('only the requested built-in voice is fetched and its Blob is explicitly released', async () => {
  const originalFetch = globalThis.fetch, requests: string[] = [];
  globalThis.fetch = async input => { requests.push(String(input)); return new Response(new Uint8Array([1, 2, 3])); };
  try {
    assert.equal(requests.length, 0);
    const source = await loadVoiceSource('/voices/welcome.wav', () => assert.fail('bundled voice must not use asset://'));
    assert.deepEqual(requests, ['/voices/welcome.wav']);
    assert.equal((await originalFetch(source.url)).headers.get('content-type'), 'audio/wav');
    source.release(); await assert.rejects(originalFetch(source.url));
    assert.equal(builtinVoicePath('goodbye'), '/voices/goodbye.wav'); assert.equal(builtinVoicePath('../welcome'), undefined);
    const unknown = await loadVoiceSource('/voices/not-bundled.wav', path => `asset:${path}`);
    assert.equal(requests[1], 'asset:/voices/not-bundled.wav'); unknown.release();
  } finally { globalThis.fetch = originalFetch; }
});
test('Quiet Mode and an explicitly unbound built-in voice never fetch or allocate audio', async () => {
  const player = new VoiceManager();
  const loader = () => assert.fail('disabled voice must not load');
  await player.play('welcome', { voices, quietMode: true } as Settings, loader, new AbortController().signal);
  await player.play('welcome', { voices: {}, quietMode: false } as Settings, loader, new AbortController().signal);
});
test('voice rows expose built-in restoration without silently re-enabling disabled or imported voices', () => {
  const dialogues = JSON.parse(readFileSync(new URL('../public/dialogues.json', import.meta.url), 'utf8')) as DialogueFile;
  const settings = { voices: { ...voices, welcome: '/local/custom.wav' } } as Settings;
  delete settings.voices.goodbye;
  const rows = voiceRows(dialogues, settings);
  assert.equal(rows.find(row => row.id === 'welcome')?.path, '/local/custom.wav');
  assert.equal(rows.find(row => row.id === 'welcome')?.builtinFile, '/voices/welcome.wav');
  assert.equal(rows.find(row => row.id === 'goodbye')?.path, undefined);
  assert.equal(rows.find(row => row.id === 'goodbye')?.builtinFile, '/voices/goodbye.wav');
  assert.equal(settings.voices.goodbye, undefined);
});
