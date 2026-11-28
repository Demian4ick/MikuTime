import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import catalog from '../src/lib/wallpaper-catalog.json' with { type: 'json' };
import { builtinWallpapers, backgroundSource } from '../src/lib/wallpapers.ts';
import { imageDimensions } from '../src/lib/appearance.ts';

test('nine bundled wallpapers have local originals and small previews, with stable unique IDs', () => {
  assert.equal(builtinWallpapers.length, 9);
  assert.equal(new Set(catalog.map(item => item.id)).size, 9);
  assert.equal(new Set(catalog.map(item => item.path)).size, 9);
  let total = 0;
  for (const item of catalog) {
    assert.match(item.id, /^builtin-[a-z-]+$/);
    assert.match(item.name, /^[А-Яа-яЁё ]+$/);
    assert.match(item.path, /^\/wallpapers\/[a-z-]+\.jpg$/);
    assert.match(item.thumbnail, /^\/wallpapers\/previews\/[a-z-]+\.jpg$/);
    assert.ok(item.sourceFile);
    const image = readFileSync(new URL(`../public${item.path}`, import.meta.url));
    const preview = readFileSync(new URL(`../public${item.thumbnail}`, import.meta.url));
    assert.equal(createHash('sha256').update(image).digest('hex'), item.sha256);
    imageDimensions(image);
    assert.deepEqual(imageDimensions(preview), { width: 320, height: 180 });
    assert.ok(preview.length < 256 * 1024);
    total += image.length + preview.length;
  }
  assert.ok(total < 512 * 1024);
});

test('only known built-in backgrounds use bundled URLs; personal files keep the local asset protocol', () => {
  const converted: string[] = [];
  const asset = (path: string) => { converted.push(path); return `asset:${path}`; };
  assert.equal(backgroundSource(null, asset), '/background.jpg');
  assert.equal(backgroundSource(undefined, asset), '/background.jpg');
  for (const item of builtinWallpapers) assert.equal(backgroundSource(item.path, asset), item.path);
  assert.deepEqual(converted, []);
  for (const path of ['/assets/personal.jpg', 'C:\\assets\\personal.jpg', '/wallpapers/unknown.jpg', 'https://example.com/image.jpg']) {
    assert.equal(backgroundSource(path, asset), `asset:${path}`);
  }
  assert.equal(converted.length, 4);
});

test('bundled cards are always present and have no remove action or eager image loading', () => {
  const source = readFileSync(new URL('../src/components/AppearanceSettings.svelte', import.meta.url), 'utf8');
  const builtinCards = source.slice(source.indexOf('{#each builtinWallpapers'), source.indexOf('{#each settings.wallpapers'));
  assert.match(builtinCards, /select\(wallpaper.id\)/);
  assert.match(builtinCards, /loading="lazy"/);
  assert.match(builtinCards, /wallpaper.thumbnail/);
  assert.doesNotMatch(builtinCards, /(?:askRemove|convertFileSrc|Убрать)/);
});
