import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { emotionNames, talkativenessNames, idleStatusText } from '../src/lib/localization.ts';

test('all emotion, chatter, and idle labels are Russian; internal IDs stay unchanged', () => {
  assert.equal(Object.keys(emotionNames).length, 10);
  for (const label of [...Object.values(emotionNames), ...Object.values(talkativenessNames)]) {
    assert.match(label, /[А-Яа-яЁё]/); assert.doesNotMatch(label, /[A-Za-z]/);
  }
  assert.equal(idleStatusText('available'), 'Доступно');
  assert.match(idleStatusText('unavailable: unsupported'), /Недоступно/);
});
test('visible UI has no old English labels or English subtitle rendering', () => {
  const app = readFileSync(new URL('../src/App.svelte', import.meta.url), 'utf8');
  const timer = readFileSync(new URL('../src/components/TimerControls.svelte', import.meta.url), 'utf8');
  const diary = readFileSync(new URL('../src/components/Diary.svelte', import.meta.url), 'utf8');
  const settings = readFileSync(new URL('../src/components/Settings.svelte', import.meta.url), 'utf8');
  for (const old of ['Focus Room', 'Activity Diary', '>Settings', 'Quiet Mode', '>Companion', 'Hatsune Miku', 'SKIP', '{visual.en}']) assert.ok(!app.includes(old), old);
  for (const old of ['>▶ Start', '>Ⅱ Pause', '>▶ Resume', '>✓ Finish', 'Skip Break', '↺ Reset']) assert.ok(!timer.includes(old), old);
  for (const old of ['>Today<', '>Statistics<', '>Calendar<', 'Activity Diary']) assert.ok(!diary.includes(old), old);
  for (const old of ['>Quiet<', '>Balanced<', '>Chatty<', '>Disabled<', 'Quiet Mode', 'Always on Top', 'System idle', 'WebKit renderer']) assert.ok(!settings.includes(old), old);
  assert.match(timer, /FocusDuration/);
  assert.doesNotMatch(app, /୨୧|Немного времени, немного волшебства|Маленький шаг навстречу мечте|Наша маленькая комната/);
  assert.doesNotMatch(settings, /୨୧|Маленький уютный мир|Сохранить ♡/);
  assert.doesNotMatch(diary, /Маленькие шаги — большие мечты|Твои маленькие шаги|тёплые победы/);
  assert.match(app, /Miku <b>Focus/);
  assert.match(settings, /Скорость появления символов/);
  const duration = readFileSync(new URL('../src/components/FocusDuration.svelte', import.meta.url), 'utf8');
  assert.doesNotMatch(duration, /<input|<form|NumberSetting/);
  assert.equal((duration.match(/<button\b/g) ?? []).length, 2);
  assert.match(duration, /Увеличить время работы на минуту/);
  assert.match(duration, /Уменьшить время работы на минуту/);
  assert.match(timer, /class="timer-readout"/);
  assert.doesNotMatch(readFileSync(new URL('../src/style.css', import.meta.url), 'utf8'), /୨୧/);
  const config = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
  assert.equal(config.app.windows[0].title, 'Miku Focus');
  const panel = readFileSync(new URL('../src/components/DialoguePanel.svelte', import.meta.url), 'utf8');
  assert.match(panel, /aria-label="Продолжить диалог"/);
  assert.doesNotMatch(panel, /visual\.en/);
  assert.match(app, /\/companion\/SmartyMiku\.png/);
  assert.match(app, /aria-label="Изменить размер помощника"/);
});
