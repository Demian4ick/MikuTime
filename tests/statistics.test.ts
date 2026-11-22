import { test } from 'node:test';
import assert from 'node:assert/strict';
import { splitDays, duration, dayKey, formatTimer, csvCell, StatisticsService } from '../src/lib/statistics.ts';
import type { Session, Timer } from '../src/lib/types.ts';
import { emotionFiles } from '../src/lib/emotions.ts';
import { validateDialogues } from '../src/lib/dialogues.ts';
import { readFileSync } from 'node:fs';

test('splits at local midnight and excludes negative segments', () => {
  const a = new Date(2026, 10, 15, 23, 55).getTime(), b = new Date(2026, 10, 16, 0, 5).getTime();
  const days = splitDays({ startMs: a, endMs: b });
  assert.equal(days.get('2026-11-15'), 300000); assert.equal(days.get('2026-11-16'), 300000);
  assert.equal(duration([{ startMs: 10, endMs: 2 }]), 0);
});
test('timer formatting rounds remaining time upwards', () => {
  assert.equal(formatTimer(1001), '00:02'); assert.equal(formatTimer(0), '00:00'); assert.equal(formatTimer(300000), '05:00');
});
test('local calendar splitting handles 23h and 25h DST days', () => {
  const previous = process.env.TZ;
  process.env.TZ = 'America/New_York';
  try {
    const spring = splitDays({ startMs: new Date(2026, 2, 8).getTime(), endMs: new Date(2026, 2, 9).getTime() });
    const autumn = splitDays({ startMs: new Date(2026, 10, 1).getTime(), endMs: new Date(2026, 10, 2).getTime() });
    assert.equal(spring.get('2026-03-08'), 23 * 3600000);
    assert.equal(autumn.get('2026-11-01'), 25 * 3600000);
  } finally { if (previous === undefined) delete process.env.TZ; else process.env.TZ = previous; }
});
test('CSV escapes quotes and spreadsheet formulas', () => {
  assert.equal(csvCell('a"b'), '"a""b"'); assert.equal(csvCell('=CMD()'), '"\'=CMD()"');
});
test('statistics include active work only; no pause or decision waiting time', () => {
  const start = new Date(2026, 10, 15, 12).getTime();
  const session: Session = { id: '1', categoryId: 'reading', startedAt: start, endedAt: null,
    completed: false, extensions: 2, segments: [{ startMs: start, endMs: start + 60000 }] };
  const timer: Timer = { status: 'AwaitingDecision', phase: 'Focus', remainingMs: 0, deadlineMs: null, segmentStartedAt: null, session, cycle: 0 };
  const stats = new StatisticsService({ sessions: [], breaks: [], timer, now: start + 999999 });
  assert.equal(stats.totalWorkMs, 60000); assert.equal(stats.get(dayKey(start)).completed, 0);
  assert.equal(stats.get(dayKey(start)).categories.reading, 60000);
  timer.status = 'Focus'; timer.segmentStartedAt = start + 120000; timer.deadlineMs = start + 180000;
  assert.equal(new StatisticsService({ sessions: [], breaks: [], timer, now: start + 999999 }).totalWorkMs, 120000);
});
test('periods aggregate all days in a month', () => {
  const d = new Date(2026, 10, 15).getTime();
  const timer: Timer = { status: 'Idle', phase: 'Focus', remainingMs: 0, deadlineMs: null, segmentStartedAt: null, session: null, cycle: 0 };
  const stats = new StatisticsService({ sessions: [{ id: 's', categoryId: 'x', startedAt: d, endedAt: d + 60000, segments: [{ startMs: d, endMs: d + 60000 }], extensions: 0, completed: true }], breaks: [], timer, now: d });
  assert.equal(stats.series('months', d).at(-1)?.ms, 60000); assert.equal(stats.averageMs, 60000);
});
test('average session duration excludes unfinished partial records', () => {
  const d = new Date(2026, 10, 15).getTime();
  const timer: Timer = { status: 'Idle', phase: 'Focus', remainingMs: 0, deadlineMs: null, segmentStartedAt: null, session: null, cycle: 0 };
  const base: Session = { id: 's', categoryId: 'x', startedAt: d, endedAt: d + 60000, segments: [{ startMs: d, endMs: d + 60000 }], extensions: 0, completed: true };
  const stats = new StatisticsService({ sessions: [base, { ...base, id: 'partial', completed: false }], breaks: [], timer, now: d });
  assert.equal(stats.totalWorkMs, 120000); assert.equal(stats.averageMs, 60000);
});
test('all supplied sprite filenames retain their exact case', () => {
  assert.equal(emotionFiles.Pleased, 'pleased.png'); assert.equal(emotionFiles.Relief, 'relief.png');
  assert.equal(emotionFiles.Dissatisfied, 'dissatisfied.png'); assert.equal(Object.keys(emotionFiles).length, 10);
});
test('editable dialogue manifest and required first-extension sequence', () => {
  const data = validateDialogues(JSON.parse(readFileSync(new URL('../public/dialogues.json', import.meta.url), 'utf8')));
  const first = data.dialogues.find(d => d.event === 'first_extension')!;
  assert.deepEqual(first.lines.map(l => l.emotion), ['Surprised', 'Pleased']);
  assert.equal(first.lines[0].en, 'Huh? Five more minutes, Senpai?');
  assert.equal(first.lines[1].ru, 'Хе-хе! Ты такой старательный. Я горжусь тобой~ ♡');
  assert.throws(() => validateDialogues({ version: 2 }));
});
test('Senpai stays selective; the requested English finish address does not change Russian subtitles', () => {
  const data = validateDialogues(JSON.parse(readFileSync(new URL('../public/dialogues.json', import.meta.url), 'utf8')));
  const lines = data.dialogues.flatMap(dialogue => dialogue.lines);
  const addressed = ['extra_surprised', 'focus_complete', 'goal', 'record', 'return', 'welcome'];
  assert.equal(data.dialogues.length, 20); assert.equal(lines.length, 21);
  assert.deepEqual(lines.filter(line => /Senpai/.test(line.en)).map(line => line.voice).sort(), ['finish', ...addressed].sort());
  assert.deepEqual(lines.filter(line => /сэмпай/i.test(line.ru)).map(line => line.voice).sort(), addressed);
  assert.equal(new Set(lines.map(line => line.voice)).size, 21);
});
test('the seven revised English scripts match the requested wording exactly', () => {
  const data = validateDialogues(JSON.parse(readFileSync(new URL('../public/dialogues.json', import.meta.url), 'utf8')));
  const expected: Record<string, string> = {
    welcome: "Welcome back, Senpai! Let's make some time for your dreams!",
    start: "One little step at a time. I'm right here with you!",
    extension: "Five more minutes? Hehe, I'm cheering for you!",
    finish: "Great work, Senpai! Now relax your shoulders, have some water, and get some rest. I'm right here with you!",
    'break-skipped': "Done resting already? Okay, but don't push yourself too hard. I'm here for you!",
    goal: "You reached today's goal! I'm so proud of you, Senpai!",
    touch: "Hmm? Looking at me instead of working? Just kidding!",
  };
  for (const [id, en] of Object.entries(expected)) {
    assert.equal(data.dialogues.find(dialogue => dialogue.id === id)?.lines[0].en, en, id);
  }
});
