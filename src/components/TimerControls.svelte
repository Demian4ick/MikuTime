<script lang="ts">
  import type { Snapshot } from '../lib/types';
  import { formatTimer } from '../lib/statistics';
  import FocusDuration from './FocusDuration.svelte';
  let { snapshot, act, adjustFocusDuration, compact = false }: { snapshot: Snapshot; act: (action: string) => void; adjustFocusDuration: (delta: -1 | 1) => Promise<void>; compact?: boolean } = $props();
  let t = $derived(snapshot.timer);
  let resting = $derived(t.phase !== 'Focus');
  let phaseLabel = $derived(resting ? (t.phase === 'LongBreak' ? 'Длинный отдых' : 'Короткий отдых') : 'Работа');
  let statusLabel = $derived(resting ? 'Идёт отдых' : ({ Idle: 'Готов к запуску', Focus: 'Идёт работа', Paused: 'Работа на паузе', Break: '', AwaitingDecision: 'Время работы истекло', Completed: 'Сессия завершена' })[t.status]);
</script>

<section class:compact class:resting class="timer-panel" aria-label={resting ? 'Таймер отдыха' : 'Таймер работы'}>
  <div class="eyebrow">{phaseLabel} <span>♡</span></div>
  <div class="timer-readout">
    <div class="timer-digits" aria-live="off">{formatTimer(snapshot.remainingMs)}</div>
    <FocusDuration value={snapshot.settings.focusMinutes} locked={resting || (t.status !== 'Idle' && t.status !== 'Completed')} adjust={adjustFocusDuration}/>
  </div>
  <p class="timer-status" aria-live="polite">{statusLabel}</p>
  {#if !resting && t.session?.extensions}<small>+{t.session.extensions * 5} мин · одна сессия</small>{/if}
  <div class="timer-buttons">
    {#if resting}
      <button class="gloss" onclick={() => act('skip')}>Закончить отдых »</button>
    {:else}
      {#if t.status === 'Idle' || t.status === 'Completed'}
        <button class="gloss primary" onclick={() => act('start')}>▶ Начать работу</button>
      {:else if t.status === 'Focus'}
        <button class="gloss primary" onclick={() => act('pause')}>Ⅱ Пауза</button>
      {:else if t.status === 'Paused'}
        <button class="gloss primary" onclick={() => act('resume')}>▶ Продолжить</button>
      {/if}
      {#if t.status === 'AwaitingDecision'}
        <button class="gloss pink" onclick={() => act('extend')}>Ещё 5 минут ♡</button>
        <button class="gloss primary" onclick={() => act('finish')}>Завершить и отдохнуть</button>
      {:else if t.session}
        <button class="gloss" onclick={() => act('finish')}>✓ Завершить</button>
      {/if}
      {#if t.status !== 'Idle'}<button class="text-button" onclick={() => act('reset')}>↺ Сбросить</button>{/if}
    {/if}
  </div>
  {#if !compact}<div class="cycle"><div>{resting ? 'Следующая сессия' : 'Сессия'} {t.cycle % snapshot.settings.sessionsBeforeLong + 1} / {snapshot.settings.sessionsBeforeLong} <span>✧</span></div><button class="text-button" disabled={t.cycle === 0} title="Начать отсчёт сессий с первой. Текущий таймер и история сохранятся." onclick={() => act('reset_cycle')}>↺ Сбросить сессии</button></div>{/if}
</section>

<style>
  .timer-readout{display:flex;align-items:center;justify-content:center;gap:8px;margin:3px 0 8px}
  .cycle{display:flex;align-items:center;justify-content:center;gap:4px;flex-wrap:wrap}
  .cycle .text-button{padding:2px 3px;font-size:9px;letter-spacing:0}
</style>
