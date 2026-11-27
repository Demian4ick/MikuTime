import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';

test('README shows the gallery immediately in a labelled 2×2 grid with full-size links', () => {
  const readme = readFileSync(new URL('../README.md', import.meta.url), 'utf8');
  const start = readme.indexOf('## Скриншоты');
  const end = readme.indexOf('## Установка');
  assert.ok(start > 0 && end > start);
  assert.doesNotMatch(readme, /<\/?(?:details|summary)\b/i);
  assert.ok(readme.indexOf('⬇ Скачать бета-версию') < readme.indexOf('docs/screenshots/focus.webp'));
  assert.ok(readme.indexOf('docs/screenshots/focus.webp') < start);
  const gallery = readme.slice(start, end);
  assert.equal((gallery.match(/<img\b/g) ?? []).length, 4);
  assert.equal((gallery.match(/<tr>/g) ?? []).length, 4);
  assert.equal((gallery.match(/<th>/g) ?? []).length, 4);
  assert.equal((gallery.match(/<td\b/g) ?? []).length, 4);
  for (const name of ['statistics', 'calendar', 'voices', 'companion']) {
    assert.ok(gallery.includes(`href="docs/screenshots/${name}.webp"`));
    assert.ok(gallery.includes(`src="docs/screenshots/${name}.webp"`));
  }
});

test('README screenshots are complete, small lossless WebP files without private metadata', () => {
  const root = new URL('../docs/screenshots/', import.meta.url);
  const names = ['calendar', 'companion', 'focus', 'statistics', 'voices'];
  assert.deepEqual(readdirSync(root).sort(), ['README.md', ...names.map(name => `${name}.webp`)].sort());
  const readme = readFileSync(new URL('../README.md', import.meta.url), 'utf8');
  assert.match(readme, /демонстрационных данных/);
  let total = 0;
  for (const name of names) {
    assert.ok(readme.includes(`docs/screenshots/${name}.webp`), name);
    const bytes = readFileSync(new URL(`${name}.webp`, root));
    total += bytes.length;
    assert.equal(bytes.toString('ascii', 0, 4), 'RIFF');
    assert.equal(bytes.toString('ascii', 8, 12), 'WEBP');
    assert.equal(bytes.readUInt32LE(4) + 8, bytes.length);
    let lossless = false;
    for (let offset = 12; offset + 8 <= bytes.length;) {
      const kind = bytes.toString('ascii', offset, offset + 4);
      const size = bytes.readUInt32LE(offset + 4);
      assert.ok(offset + 8 + size <= bytes.length);
      assert.ok(!['EXIF', 'XMP ', 'ICCP', 'ANIM', 'VP8 '].includes(kind), `${name}: ${kind}`);
      if (kind === 'VP8L') {
        lossless = true;
        assert.equal(bytes[offset + 8], 0x2f);
        const header = bytes.readUInt32LE(offset + 9);
        assert.equal((header & 0x3fff) + 1, name === 'companion' ? 240 : 1000);
        assert.equal(((header >>> 14) & 0x3fff) + 1, name === 'companion' ? 260 : 760);
        if (name === 'companion') assert.equal((header >>> 28) & 1, 1, 'Keep original window transparency');
      }
      offset += 8 + size + (size % 2);
    }
    assert.ok(lossless, `${name} must be lossless`);
  }
  assert.ok(total < 700000, 'Keep the README gallery lightweight');
});
