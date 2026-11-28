import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { AppearanceWrites, applyTheme, encodeWallpaper, imageDimensions, imageLimit, previewLimit, wallpaperName } from '../src/lib/appearance.ts';

test('appearance queue waits for a complete batch and allows exit after a rejected image was handled', async () => {
  const queue = new AppearanceWrites(), writes: number[] = [];
  let release!: () => void;
  const batch = queue.run(async () => { writes.push(1); await new Promise<void>(resolve => release = resolve); writes.push(2); });
  const next = queue.run(async () => { writes.push(3); });
  await Promise.resolve();
  let flushed = false;
  const flush = queue.flush().then(() => flushed = true);
  assert.deepEqual(writes, [1]); assert.equal(flushed, false);
  release(); await Promise.all([batch, next, flush]); assert.deepEqual(writes, [1, 2, 3]);
  await assert.rejects(queue.run(async () => { throw new Error('invalid image'); }), /invalid image/);
  await queue.flush();
});
test('a failed write still cancels an exit which was waiting for that write', async () => {
  const queue = new AppearanceWrites(); let release!: () => void;
  const write = queue.run(async () => { await new Promise<void>(resolve => release = resolve); throw new Error('disk'); });
  await Promise.resolve(); const flush = queue.flush();
  const checks = Promise.all([assert.rejects(write, /disk/), assert.rejects(flush, /disk/)]);
  release(); await checks;
});

test('theme changes are local DOM attributes and identical clock updates do not write the DOM', () => {
  let value: string | undefined, writes = 0;
  const root = { dataset: { get theme() { return value; }, set theme(next: string | undefined) { value = next; writes++; } } };
  applyTheme('Dark', root); assert.equal(value, 'dark');
  for (let i = 0; i < 100; i++) applyTheme('Dark', root);
  assert.equal(writes, 1);
  applyTheme('Light', root); assert.equal(value, 'light'); assert.equal(writes, 2);
});
test('wallpaper names are readable bounded file names, not paths or control characters', () => {
  assert.equal(wallpaperName('C:\\Pictures\\Ночной город.jpeg'), 'Ночной город');
  assert.equal(wallpaperName('/images/\u0000\n.png'), 'Обои');
  assert.equal(Array.from(wallpaperName('я'.repeat(100) + '.png')).length, 80);
});
test('large and malformed image headers are rejected before a bitmap can be allocated', () => {
  const png = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 7, 128, 0, 0, 4, 56]);
  assert.deepEqual(imageDimensions(png), { width: 1920, height: 1080 });
  new DataView(png.buffer).setUint32(16, 4097);
  assert.throws(() => imageDimensions(png), /4096/);
  assert.deepEqual(imageDimensions(new Uint8Array([255, 216, 255, 192, 0, 8, 8, 2, 208, 5, 0, 1])), { width: 1280, height: 720 });
  for (const bytes of [new Uint8Array(), new Uint8Array([255, 216, 255, 192]), new Uint8Array([255, 216, 255, 224, 0, 1])]) assert.throws(() => imageDimensions(bytes), /изображение/);
});
test('binary wallpaper payload preserves originals and separates a bounded thumbnail without JSON byte arrays', () => {
  const original = new Uint8Array([1, 2, 3, 4]), preview = new Uint8Array([5, 6]);
  const payload = encodeWallpaper('Обои', original, preview);
  const length = new DataView(payload.buffer).getUint32(0, true);
  assert.deepEqual(JSON.parse(new TextDecoder().decode(payload.slice(4, 4 + length))), { name: 'Обои', previewLength: 2 });
  assert.deepEqual(payload.slice(4 + length, 6 + length), preview);
  assert.deepEqual(payload.slice(6 + length), original);
  assert.throws(() => encodeWallpaper('Обои', new Uint8Array(0), preview));
  assert.throws(() => encodeWallpaper('Обои', new Uint8Array(imageLimit + 1), preview));
  assert.throws(() => encodeWallpaper('Обои', original, new Uint8Array(previewLimit + 1)));
});
test('dark theme covers both windows without filtering character images or changing the transparent root', () => {
  const css = readFileSync(new URL('../src/themes.css', import.meta.url), 'utf8');
  for (const surface of ['.app-shell', '.settings', '.diary', '.dialogue-panel', '.timer-panel', '.sound-table', '.category-menu', '.companion-clock', '.companion-bubble', '.companion-menu', '.toast', '.json-editor', '.info-note']) assert.ok(css.includes(surface));
  assert.doesNotMatch(css, /(?:filter:\s*(?:invert|brightness)|data-theme=dark\]\s*(?:img|body)[\s{])/);
  const component = readFileSync(new URL('../src/components/AppearanceSettings.svelte', import.meta.url), 'utf8');
  assert.match(component, /loading="lazy"/);
  assert.match(component, /wallpaper\.thumbnail/);
  assert.match(component, /aria-pressed/);
});
