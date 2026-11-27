import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const code = readFileSync(new URL('../integrations/obsidian/Панель/view.js', import.meta.url), 'utf8');
const { mikuSnapshot, mikuModel, mikuSplit, mikuDayKey, mikuMidnight } = await new AsyncFunction('dv', 'input', `${code}\nreturn { mikuSnapshot, mikuModel, mikuSplit, mikuDayKey, mikuMidnight };`)(undefined, undefined);
const now = new Date(2026, 10, 23, 12).getTime();
const minute = 60000;
const segment = (start: number, end: number) => ({ startMs: start, endMs: end });
function snapshot(os = 'linux', id = os) {
  return { format: 'miku-focus-statistics', schemaVersion: 1, source: { id, os }, capturedAt: now, exportedAt: now,
    categories: [{ id: 'work', name: 'Работа', color: '#59c9c2' }], currentSessionId: 's1',
    sessions: [{ id: 's1', categoryId: 'work', startedAt: now - 10 * minute, endedAt: null as number | null, completed: false, extensions: 1, segments: [segment(now - 10 * minute, now)] }],
    breaks: [segment(now - 20 * minute, now - 15 * minute)] };
}

test('Obsidian dashboard accepts only the expected v1 source and rejects corrupt segments', () => {
  const data = snapshot(); assert.equal(mikuSnapshot(data, 'linux'), data);
  assert.throws(() => mikuSnapshot(data, 'windows'));
  assert.throws(() => mikuSnapshot({ ...data, schemaVersion: 2 }, 'linux'));
  assert.throws(() => mikuSnapshot({ ...data, sessions: [{ ...data.sessions[0], segments: [segment(now, now - 1)] }] }, 'linux'));
  assert.throws(() => mikuSnapshot({ ...data, sessions: [{ ...data.sessions[0], segments: [segment(NaN, now)] }] }, 'linux'));
});
test('duplicate session/category IDs make a snapshot invalid instead of inflating totals', () => {
  const data = snapshot();
  assert.throws(() => mikuSnapshot({ ...data, sessions: [...data.sessions, ...data.sessions] }, 'linux'));
  assert.throws(() => mikuSnapshot({ ...data, categories: [...data.categories, ...data.categories] }, 'linux'));
});
test('current session is counted once; no offline or post-snapshot work is credited', () => {
  const data = snapshot(); data.sessions[0].segments = [segment(now - 10 * minute, now + 60 * minute)];
  const model = mikuModel([data], '30', now + 120 * minute);
  assert.equal(model.workMs, 10 * minute); assert.equal(model.completed, 0); assert.equal(model.breakMs, 5 * minute);
});
test('independent OS sources add up but another copy of the same source does not', () => {
  const linux = snapshot(), windows = snapshot('windows');
  const model = mikuModel([linux, windows, structuredClone(linux)], '30', now);
  assert.equal(model.workMs, 20 * minute); assert.equal(model.sources.length, 2); assert.equal(model.activeDays, 1);
});
test('identically named categories stay separate across installations', () => {
  const model = mikuModel([snapshot(), snapshot('windows')], '30', now);
  assert.equal(model.categories.length, 2); assert.notEqual(model.categories[0].key, model.categories[1].key);
});
test('deleted category metadata still labels and counts historical work in Obsidian', () => {
  const data = snapshot();
  const deleted = { ...data, categories: data.categories.map(category => ({ ...category, archived: true, deleted: true })) };
  assert.equal(mikuSnapshot(deleted, 'linux'), deleted);
  const before = mikuModel([data], 'all', now), after = mikuModel([deleted], 'all', now);
  assert.equal(after.workMs, before.workMs);
  assert.deepEqual(after.categories, before.categories);
});
test('a segment crossing midnight is split by local days, not by session start', () => {
  const midnight = mikuMidnight(now), data = snapshot();
  data.sessions[0].segments = [segment(midnight - 15 * minute, midnight + 10 * minute)];
  const model = mikuModel([data], '7', now);
  assert.equal(model.days.get(mikuDayKey(midnight - 1)).linux, 15 * minute);
  assert.equal(model.days.get(mikuDayKey(midnight)).linux, 10 * minute);
  assert.equal(model.activeDays, 2);
});
test('period clipping includes only the portion of a segment inside the selected calendar days', () => {
  const lower = mikuMidnight(now, -6), data = snapshot();
  data.sessions[0].segments = [segment(lower - minute, lower + 2 * minute)];
  data.breaks = [segment(lower - minute, lower + minute)];
  const model = mikuModel([data], '7', now);
  assert.equal(model.workMs, 2 * minute); assert.equal(model.breakMs, minute);
});
test('completed sessions use completion date while partial/current sessions remain excluded', () => {
  const data = snapshot();
  data.sessions[0].endedAt = now - minute; data.sessions[0].completed = true;
  assert.equal(mikuModel([data], '7', now).completed, 1);
  data.sessions[0].endedAt = mikuMidnight(now, -7);
  assert.equal(mikuModel([data], '7', now).completed, 0);
  assert.equal(mikuModel([data], 'all', now).completed, 1);
});
test('spring/fall daylight-saving days are split at real local midnights', () => {
  const before = process.env.TZ;
  try {
    process.env.TZ = 'Europe/Berlin';
    for (const [month, day, hours] of [[2, 29, 23], [9, 25, 25]]) {
      const start = new Date(2026, month, day).getTime(), end = new Date(2026, month, day + 1).getTime();
      const pieces: number[] = []; mikuSplit(start, end, (_day: string, ms: number) => pieces.push(ms));
      assert.deepEqual(pieces, [hours * 3600000]);
    }
  } finally { if (before === undefined) delete process.env.TZ; else process.env.TZ = before; }
});
test('missing sources create an honest empty view without fabricated Windows results', () => {
  const model = mikuModel([], '30', now);
  assert.equal(model.workMs, 0); assert.equal(model.sources.length, 0); assert.equal(model.bestDay, undefined);
  assert.equal(model.categories.length, 0); assert.equal(model.days.size, 0);
});
test('Obsidian view is local, text-safe and event-driven with component cleanup', () => {
  assert.ok(!code.includes('innerHTML')); assert.ok(!code.includes('fetch('));
  assert.ok(!code.includes('setInterval(')); assert.ok(!code.includes('setTimeout('));
  assert.ok(!code.includes('vault.modify(')); assert.ok(!code.includes('vault.create('));
  assert.ok(code.includes('element.textContent = text'));
  assert.ok(code.includes('dv.component.register('));
  assert.ok(code.includes('dv.app.vault.offref(event)'));
  assert.ok(code.includes('disposed = true'));
});
