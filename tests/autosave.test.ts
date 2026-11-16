import { test } from 'node:test';
import assert from 'node:assert/strict';
import { AutoSaver } from '../src/lib/autosave.ts';
import { advanceCharacters } from '../src/lib/typing.ts';

test('autosave serializes writes and merges rapid edits without losing other fields', async () => {
  const writes: object[] = [];
  let release!: () => void;
  const saver = new AutoSaver<{ minutes?: number; volume?: number }>(async patch => {
    writes.push(patch);
    if (writes.length === 1) await new Promise<void>(resolve => release = resolve);
  });
  const a = saver.save({ minutes: 1 });
  const b = saver.save({ minutes: 12 });
  const c = saver.save({ volume: 0.5 });
  const d = saver.save({ minutes: 123 });
  assert.deepEqual(writes, [{ minutes: 1 }]);
  let flushed = false;
  const flushing = saver.flush().then(() => flushed = true);
  await Promise.resolve(); assert.equal(flushed, false);
  release(); await Promise.all([a, b, c, d, flushing]);
  assert.deepEqual(writes, [{ minutes: 1 }, { minutes: 123, volume: 0.5 }]);
  assert.equal(flushed, true);
});
test('failed autosave reports an error and accepts subsequent corrections', async () => {
  let fail = true;
  const statuses: string[] = [];
  const saver = new AutoSaver<{ value: number }>(async () => { if (fail) throw new Error('disk'); }, status => statuses.push(status));
  await assert.rejects(saver.save({ value: 1 }), /disk/);
  await assert.rejects(saver.flush(), /disk/);
  fail = false; await saver.save({ value: 2 }); await saver.flush();
  assert.deepEqual(statuses, ['saving', 'error', 'saving', 'saved']);
});
test('text speed is frame-rate independent, supports instant mode and caps the line', () => {
  assert.equal(advanceCharacters(0, 1000, 10, 100), 10);
  assert.equal(advanceCharacters(0, 1000, 120, 100), 100);
  assert.equal(advanceCharacters(2, 0, 0, 100), 100);
  assert.equal(advanceCharacters(2, -1, 60, 100), 2);
  let cursor = 0;
  for (let i = 0; i < 100; i++) cursor = advanceCharacters(cursor, 10, 5, 100);
  assert.ok(Math.abs(cursor - 5) < 1e-10);
});
