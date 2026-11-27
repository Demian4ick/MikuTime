import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';

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
