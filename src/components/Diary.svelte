<script lang="ts">
  import type { Snapshot } from '../lib/types';
  import { StatisticsService, formatDuration, duration, dayKey } from '../lib/statistics';
  let { snapshot, exportData }: { snapshot: Snapshot; exportData: (format: string) => void } = $props();
  let tab = $state<'today' | 'statistics' | 'calendar'>('today');
  let period = $state<'days' | 'weeks' | 'months'>('days');
  let selected = $state(dayKey(Date.now()));
  let month = $state(new Date(new Date().getFullYear(), new Date().getMonth(), 1));
  let stats = $derived(new StatisticsService(snapshot));
  let today = $derived(stats.get(dayKey(snapshot.now)));
  let detail = $derived(stats.get(tab === 'today' ? dayKey(snapshot.now) : selected));
  let series = $derived(stats.series(period, snapshot.now));
  let maxBar = $derived(Math.max(...series.map(d => d.ms), 60_000));
  let monthCells = $derived.by(() => {
    const first = (month.getDay() + 6) % 7;
    const count = new Date(month.getFullYear(), month.getMonth() + 1, 0).getDate();
    return Array.from({ length: first + count }, (_, i) => i < first ? null : dayKey(new Date(month.getFullYear(), month.getMonth(), i - first + 1).getTime()));
  });
  function changeMonth(delta: number) { month = new Date(month.getFullYear(), month.getMonth() + delta, 1); }
  function categoryName(id: string) { return snapshot.categories.find(c => c.id === id)?.name ?? id; }
  function intensity(ms: number) { return ms === 0 ? 0 : Math.min(4, Math.ceil(ms / 3_600_000)); }
</script>

<section class="diary scroll-area">
  <div class="section-heading"><h2>Дневник занятий</h2><div class="export-buttons"><button class="gloss small" onclick={() => exportData('csv')}>Таблица CSV ↓</button><button class="gloss small" onclick={() => exportData('json')}>Данные JSON ↓</button></div></div>
  <div class="subtabs" aria-label="Разделы дневника">
    <button class:active={tab === 'today'} onclick={() => tab = 'today'}>Сегодня</button>
    <button class:active={tab === 'statistics'} onclick={() => tab = 'statistics'}>Статистика</button>
    <button class:active={tab === 'calendar'} onclick={() => tab = 'calendar'}>Календарь</button>
  </div>
  {#if tab === 'today'}
    <div class="stat-cards"><div><span>Рабочее время</span><strong>{formatDuration(today.workMs)}</strong></div><div><span>Завершено сессий</span><strong>{today.completed}</strong></div><div><span>Время отдыха</span><strong>{formatDuration(today.breakMs)}</strong></div></div>
    {#if snapshot.settings.dailyGoalMinutes > 0}<div class="goal-line"><span>Дневная цель · {formatDuration(snapshot.settings.dailyGoalMinutes * 60_000)}</span><b>{Math.min(100, Math.floor(today.workMs / (snapshot.settings.dailyGoalMinutes * 600)))}%</b><progress max={snapshot.settings.dailyGoalMinutes * 60_000} value={today.workMs}></progress></div>{/if}
  {:else if tab === 'statistics'}
    <div class="stat-cards"><div><span>Всё рабочее время</span><strong>{formatDuration(stats.totalWorkMs)}</strong></div><div><span>Средняя сессия*</span><strong>{formatDuration(snapshot.sessions.filter(s => s.completed).reduce((n, s) => n + duration(s.segments), 0) / Math.max(1, stats.completed))}</strong></div><div><span>Всего завершено</span><strong>{stats.completed}</strong></div></div>
    <div class="chart-heading"><h3>Рабочее время по периодам</h3><select aria-label="Период графика" bind:value={period}><option value="days">Дни</option><option value="weeks">Недели</option><option value="months">Месяцы</option></select></div>
    <div class="bar-chart" role="img" aria-label="Рабочее время по периодам">
      {#each series as point}<div class="bar-column" title={`${point.label}: ${formatDuration(point.ms)}`}><span class="bar" style:height={`${Math.max(2, point.ms / maxBar * 100)}%`}></span><small>{point.label}</small></div>{/each}
    </div>
    <p class="muted small-text">* Только завершённые сессии. Паузы и отдых не учитываются.</p>
    <h3>Все категории</h3>
    {#each snapshot.categories as category}
      {@const ms = [...stats.days.values()].reduce((sum, d) => sum + (d.categories[category.id] ?? 0), 0)}
      {#if ms > 0}<div class="category-stat"><span class="color-dot" style:background={category.color}></span><span>{category.name}</span><progress max={Math.max(1, stats.totalWorkMs)} value={ms}></progress><b>{formatDuration(ms)}</b></div>{/if}
    {/each}
  {:else}
    <div class="calendar-heading"><button class="gloss small" onclick={() => changeMonth(-1)} aria-label="Предыдущий месяц">‹</button><h3>{month.toLocaleDateString('ru-RU', { month: 'long', year: 'numeric' })}</h3><button class="gloss small" onclick={() => changeMonth(1)} aria-label="Следующий месяц">›</button></div>
    <div class="calendar-grid">
      {#each ['Пн', 'Вт', 'Ср', 'Чт', 'Пт', 'Сб', 'Вс'] as weekday}<span class="weekday">{weekday}</span>{/each}
      {#each monthCells as key}
        {#if key}<button class="calendar-day level-{intensity(stats.get(key).workMs)}" class:selected={selected === key} title={`${key}: ${formatDuration(stats.get(key).workMs)}`} onclick={() => selected = key}>{Number(key.slice(-2))}<small>{stats.get(key).workMs ? `${Math.floor(stats.get(key).workMs / 60000)}м` : '·'}</small></button>{:else}<span></span>{/if}
      {/each}
    </div>
    <div class="calendar-key"><span>Меньше</span>{#each [0, 1, 2, 3, 4] as level}<i class="level-{level}"></i>{/each}<span>Больше</span><b>{selected} · {formatDuration(detail.workMs)}</b></div>
  {/if}
  {#if tab !== 'statistics'}
    <div class="diary-columns"><div><h3>По категориям</h3>
      {#each Object.entries(detail.categories) as [id, ms]}<div class="category-stat"><span class="color-dot" style:background={snapshot.categories.find(c => c.id === id)?.color ?? '#3caeb0'}></span><span>{categoryName(id)}</span><b>{formatDuration(ms)}</b></div>{:else}<p class="empty-note">За этот день нет записей.</p>{/each}
    </div><div><h3>История дня <span>♡</span></h3>
      {#each detail.sessions.slice().reverse() as session}<div class="history-row"><time>{new Date(session.startedAt).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' })}</time><div><b>{categoryName(session.categoryId)}</b><small>{session.completed ? 'Завершена' : session.endedAt === null ? 'В процессе' : 'Частичная'}{session.extensions ? ` · +${session.extensions} × 5 мин` : ''}</small></div><span>{formatDuration(duration(session.segments))}</span></div>{:else}<p class="empty-note">За этот день нет сессий.</p>{/each}
    </div></div>
  {/if}
</section>
