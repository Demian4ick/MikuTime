import { test } from 'node:test';
import assert from 'node:assert/strict';
import { CategoryWrites } from '../src/lib/category-writes.ts';
import type { Category } from '../src/lib/types.ts';

const category = (id = 'work', name = 'Работа'): Category => ({ id, name, color: '#4cb9b1', archived: false });
test('category creations are separate serialized writes, including during exit flush', async () => {
  const writes: Category[] = [];
  let release!: () => void;
  const queue = new CategoryWrites(async value => {
    writes.push(value);
    if (writes.length === 1) await new Promise<void>(resolve => release = resolve);
  }, async () => {});
  const first = queue.save(category('', 'Первая'));
  const second = queue.save(category('', 'Вторая'));
  const third = queue.save(category('', 'Третья'));
  await Promise.resolve(); await Promise.resolve();
  let flushed = false;
  const flush = queue.flush().then(() => flushed = true);
  assert.equal(writes.length, 1); assert.equal(flushed, false);
  release(); await Promise.all([first, second, third, flush]);
  assert.deepEqual(writes.map(value => value.name), ['Первая', 'Вторая', 'Третья']);
  assert.equal(flushed, true);
});
test('deletion drains coalesced edits, blocks late edits/double deletion and is awaited by exit', async () => {
  const events: string[] = [];
  let release!: () => void;
  const queue = new CategoryWrites(async value => {
    events.push(value.name);
    if (events.length === 1) await new Promise<void>(resolve => release = resolve);
  }, async id => { events.push(`delete:${id}`); });
  const first = queue.save(category('work', 'Первая правка'));
  const next = queue.save(category('work', 'Вторая правка'));
  const latest = queue.save(category('work', 'Последняя правка'));
  const deleting = queue.delete('work');
  await assert.rejects(queue.save(category()), /удаляется/);
  await assert.rejects(queue.delete('work'), /удаляется/);
  let flushed = false;
  const flush = queue.flush().then(() => flushed = true);
  assert.deepEqual(events, ['Первая правка']); assert.equal(flushed, false);
  release(); await Promise.all([first, next, latest, deleting, flush]);
  assert.deepEqual(events, ['Первая правка', 'Последняя правка', 'delete:work']);
});
test('failed edit prevents deletion and can be corrected before retry', async () => {
  let fail = true, removed = 0;
  const queue = new CategoryWrites(async () => { if (fail) throw new Error('disk'); }, async () => { removed++; });
  await assert.rejects(queue.save(category()), /disk/);
  await assert.rejects(queue.delete('work'), /disk/);
  await assert.rejects(queue.flush(), /disk/);
  assert.equal(removed, 0);
  fail = false;
  await queue.save(category()); await queue.delete('work'); await queue.flush();
  assert.equal(removed, 1);
});
test('failed create/delete can be retried without dropping later creations', async () => {
  let fail = true, created = 0, removed = 0;
  const queue = new CategoryWrites(async () => { if (fail) throw new Error('disk'); created++; }, async () => { if (fail) throw new Error('disk'); removed++; });
  await assert.rejects(queue.save(category('')), /disk/);
  await assert.rejects(queue.delete('work'), /disk/);
  fail = false;
  await queue.delete('work'); await queue.save(category('')); await queue.flush();
  assert.equal(created, 1); assert.equal(removed, 1);
});
