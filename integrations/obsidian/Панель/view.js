/* Local Dataview view: read-only snapshots, no network, timers or history writes. */
const MIKU_SOURCES = [
  { os: 'linux', name: 'Arch Linux', path: 'Дневник/Статистика/FocusMikuArch/Miku Focus/Данные/linux.json', color: '#59c9c2' },
  { os: 'windows', name: 'Windows', path: 'Дневник/Статистика/FocusMikuWin/Miku Focus/Данные/windows.json', color: '#a99bf5' },
];
const MIKU_DAY = 86400000;
const mikuTime = value => Number.isSafeInteger(value) && value >= 0 && value <= 8640000000000000;
const mikuDayKey = value => {
  const date = new Date(value);
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
};
function mikuMidnight(value, offset = 0) {
  const date = new Date(value);
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + offset).getTime();
}
function mikuDuration(ms) {
  const minutes = Math.floor(ms / 60000);
  return minutes >= 60 ? `${Math.floor(minutes / 60)} ч ${minutes % 60} мин` : `${minutes} мин`;
}
function mikuSnapshot(value, os) {
  const fail = () => { throw new Error('Неизвестный или повреждённый формат выгрузки'); };
  if (!value || value.format !== 'miku-focus-statistics' || value.schemaVersion !== 1
    || value.source?.os !== os || typeof value.source.id !== 'string' || !value.source.id || value.source.id.length > 512
    || !mikuTime(value.capturedAt) || !mikuTime(value.exportedAt)
    || !Array.isArray(value.sessions) || !Array.isArray(value.categories) || !Array.isArray(value.breaks)) fail();
  const segments = list => {
    if (!Array.isArray(list)) fail();
    for (const segment of list) {
      if (!segment || !mikuTime(segment.startMs) || !mikuTime(segment.endMs) || segment.endMs < segment.startMs) fail();
    }
  };
  const ids = new Set();
  for (const session of value.sessions) {
    if (!session || typeof session.id !== 'string' || !session.id || ids.has(session.id)
      || typeof session.categoryId !== 'string' || typeof session.completed !== 'boolean'
      || !mikuTime(session.startedAt) || (session.endedAt !== null && !mikuTime(session.endedAt))) fail();
    ids.add(session.id); segments(session.segments);
  }
  const categories = new Set();
  for (const category of value.categories) {
    if (!category || typeof category.id !== 'string' || typeof category.name !== 'string' || categories.has(category.id)) fail();
    categories.add(category.id);
  }
  segments(value.breaks);
  return value;
}
function mikuSplit(start, end, visit) {
  for (let cursor = start; cursor < end;) {
    const next = Math.min(end, mikuMidnight(cursor, 1));
    if (next <= cursor) break;
    visit(mikuDayKey(cursor), next - cursor);
    cursor = next;
  }
}
function mikuModel(snapshots, period = '30', now = Date.now()) {
  const today = mikuMidnight(now);
  const lower = period === 'all' ? 0 : mikuMidnight(now, 1 - Number(period));
  const days = new Map(), categories = new Map(), sources = [], seen = new Set();
  let workMs = 0, breakMs = 0, completed = 0;
  for (const snapshot of snapshots) {
    // A second copy of a source is not another installation or another history.
    if (seen.has(snapshot.source.id)) continue;
    seen.add(snapshot.source.id);
    const os = snapshot.source.os, ceiling = Math.min(now, snapshot.capturedAt);
    const summary = { os, id: snapshot.source.id, workMs: 0, completed: 0, capturedAt: snapshot.capturedAt };
    const names = new Map(snapshot.categories.map(category => [category.id, category]));
    for (const session of snapshot.sessions) {
      let sessionMs = 0;
      for (const segment of session.segments) {
        const start = Math.max(segment.startMs, lower), end = Math.min(segment.endMs, ceiling);
        if (end <= start) continue;
        sessionMs += end - start;
        mikuSplit(start, end, (key, ms) => {
          const day = days.get(key) ?? { linux: 0, windows: 0 };
          day[os] += ms; days.set(key, day);
        });
      }
      if (sessionMs) {
        const key = JSON.stringify([snapshot.source.id, session.categoryId]);
        const category = names.get(session.categoryId);
        const item = categories.get(key) ?? { key, os, name: category?.name ?? 'Без категории', color: /^#[0-9a-f]{6}$/i.test(category?.color ?? '') ? category.color : '#59c9c2', workMs: 0 };
        item.workMs += sessionMs; categories.set(key, item);
        workMs += sessionMs; summary.workMs += sessionMs;
      }
      if (session.completed && session.endedAt !== null && session.endedAt >= lower && session.endedAt <= ceiling) {
        completed++; summary.completed++;
      }
    }
    for (const segment of snapshot.breaks) {
      breakMs += Math.max(0, Math.min(segment.endMs, ceiling) - Math.max(segment.startMs, lower));
    }
    sources.push(summary);
  }
  const activeDays = days.size;
  const bestDay = [...days].sort((a, b) => (b[1].linux + b[1].windows) - (a[1].linux + a[1].windows))[0];
  return { today, lower, workMs, breakMs, completed, activeDays, bestDay, days, sources, categories: [...categories.values()].sort((a, b) => b.workMs - a.workMs) };
}

async function mikuDashboard(dv, input) {
  // Dataview may render again on the same component without unloading it.
  // Replace the old view's subscriptions, not just its DOM.
  const stateKey = Symbol.for('miku-focus-dashboard');
  const previous = dv.container[stateKey];
  previous?.dispose();
  if (!previous) dv.component.register(() => { dv.container[stateKey]?.dispose(); delete dv.container[stateKey]; });
  const definitions = MIKU_SOURCES.filter(source => !input?.os || source.os === input.os);
  const doc = dv.container.ownerDocument;
  const make = (parent, tag, className, text) => {
    const element = doc.createElement(tag);
    if (className) element.className = className;
    if (text !== undefined) element.textContent = text;
    parent.appendChild(element); return element;
  };
  const root = make(dv.container, 'section', 'miku-stats');
  root.setAttribute('aria-label', 'Статистика Miku Focus');
  let period = '30', snapshots = [], statuses = [], disposed = false, loading = false, pending = false;
  const dateLabel = value => new Date(value).toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' });
  const sourceName = os => MIKU_SOURCES.find(source => source.os === os)?.name ?? os;

  function render() {
    if (disposed) return;
    root.replaceChildren();
    const model = mikuModel(snapshots, period);
    const header = make(root, 'header', 'miku-heading');
    const heading = make(header, 'div');
    make(heading, 'div', 'miku-eyebrow', 'Miku Focus · личная аналитика');
    make(heading, 'h2', '', input?.os ? sourceName(input.os) : 'Обзор рабочего времени');
    make(heading, 'p', 'miku-muted', 'Только фактическая работа. Отдых, паузы и ожидание решения — отдельно.');
    const refresh = make(header, 'button', 'miku-refresh', '↻ Обновить');
    refresh.onclick = () => void reload();
    const controls = make(root, 'div', 'miku-periods');
    controls.setAttribute('aria-label', 'Период статистики');
    for (const [value, name] of [['7', '7 дней'], ['30', '30 дней'], ['90', '90 дней'], ['365', 'Год'], ['all', 'Всё время']]) {
      const button = make(controls, 'button', value === period ? 'is-active' : '', name);
      button.setAttribute('aria-pressed', String(value === period));
      button.onclick = () => { period = value; render(); };
    }
    const cards = make(root, 'div', 'miku-metrics');
    for (const [label, value, detail] of [
      ['В фокусе', mikuDuration(model.workMs), 'Работа по сохранённым сегментам'],
      ['Завершено сессий', String(model.completed), 'По дате завершения, без текущих'],
      ['Активных дней', String(model.activeDays), `Среднее: ${mikuDuration(model.workMs / Math.max(1, model.activeDays))} за активный день`],
      ['Отдых', mikuDuration(model.breakMs), 'Не входит в рабочее время'],
    ]) {
      const card = make(cards, 'div', 'miku-metric');
      make(card, 'div', 'miku-muted', label); make(card, 'strong', '', model.sources.length ? value : '—'); make(card, 'small', '', detail);
    }
    const grid = make(root, 'div', 'miku-grid');
    const trend = make(grid, 'section', 'miku-card miku-trend');
    make(trend, 'h3', '', 'Ритм работы');
    const rangeEnd = model.today;
    const firstKey = [...model.days.keys()].sort()[0];
    const firstDay = firstKey ? new Date(`${firstKey}T00:00:00`).getTime() : model.today;
    const rangeStart = period === 'all' ? Math.min(firstDay, model.today) : model.lower;
    const count = Math.round((rangeEnd - rangeStart) / MIKU_DAY) + 1;
    const bucketSize = Math.max(1, Math.ceil(count / 45));
    const buckets = [];
    for (let cursor = rangeStart; cursor <= rangeEnd;) {
      const next = mikuMidnight(cursor, bucketSize), item = { start: cursor, end: Math.min(next, mikuMidnight(rangeEnd, 1)), linux: 0, windows: 0 };
      for (let day = cursor; day < item.end; day = mikuMidnight(day, 1)) {
        const values = model.days.get(mikuDayKey(day));
        if (values) { item.linux += values.linux; item.windows += values.windows; }
      }
      buckets.push(item); cursor = next;
    }
    const max = Math.max(60000, ...buckets.map(bucket => bucket.linux + bucket.windows));
    make(trend, 'p', 'miku-muted', `${bucketSize === 1 ? 'По дням' : `Столбец = ${bucketSize} дней`} · максимум ${mikuDuration(max)}`);
    const chart = make(trend, 'div', 'miku-bars');
    chart.setAttribute('role', 'group'); chart.setAttribute('aria-label', 'Рабочее время по дням и ОС');
    for (const bucket of buckets) {
      const bar = make(chart, 'div', 'miku-bar');
      const label = `${dateLabel(bucket.start)}${bucketSize > 1 ? ` — ${dateLabel(bucket.end - 1)}` : ''}: Arch Linux ${mikuDuration(bucket.linux)}, Windows ${mikuDuration(bucket.windows)}`;
      bar.title = label; bar.setAttribute('aria-label', label); bar.setAttribute('role', 'img'); bar.tabIndex = 0;
      for (const source of MIKU_SOURCES) {
        const segment = make(bar, 'div', `miku-bar-${source.os}`);
        segment.style.height = `${100 * bucket[source.os] / max}%`;
      }
    }
    const axis = make(trend, 'div', 'miku-axis');
    make(axis, 'span', '', dateLabel(rangeStart)); make(axis, 'span', '', dateLabel(rangeEnd));
    const legend = make(trend, 'div', 'miku-legend');
    for (const source of definitions) make(legend, 'span', `miku-label-${source.os}`, source.name);

    const compare = make(grid, 'section', 'miku-card');
    make(compare, 'h3', '', 'Распределение по системам');
    for (const definition of definitions) {
      const source = model.sources.find(item => item.os === definition.os);
      const status = statuses.find(item => item.os === definition.os);
      const item = make(compare, 'div', 'miku-source');
      const row = make(item, 'div', 'miku-source-title');
      make(row, 'strong', `miku-label-${definition.os}`, definition.name);
      make(row, 'strong', '', source ? mikuDuration(source.workMs) : 'Нет выгрузки');
      const track = make(item, 'div', 'miku-track');
      const fill = make(track, 'div', `miku-bar-${definition.os}`);
      fill.style.width = `${model.workMs && source ? 100 * source.workMs / model.workMs : 0}%`;
      make(item, 'small', 'miku-muted', source
        ? `Снимок: ${new Date(source.capturedAt).toLocaleString('ru-RU')}`
        : status?.error ?? 'Выберите папку выгрузки в приложении на этой ОС');
      if (source && Date.now() - source.capturedAt > 2 * MIKU_DAY) make(item, 'small', 'miku-warning', 'Снимок старше двух дней — показаны последние сохранённые данные.');
    }
    const best = make(compare, 'div', 'miku-best');
    make(best, 'span', 'miku-muted', 'Лучший день периода');
    make(best, 'strong', '', model.bestDay ? `${dateLabel(new Date(`${model.bestDay[0]}T00:00:00`).getTime())} · ${mikuDuration(model.bestDay[1].linux + model.bestDay[1].windows)}` : 'Нет рабочих сегментов');

    const heat = make(root, 'section', 'miku-card');
    make(heat, 'h3', '', 'Календарь активности');
    make(heat, 'p', 'miku-muted', 'Последние 12 недель · каждый квадрат — день · учитывается выбранный период');
    const calendar = make(heat, 'div', 'miku-calendar');
    const heatStart = mikuMidnight(model.today, -83);
    const heatMax = Math.max(60000, ...[...model.days.values()].map(day => day.linux + day.windows));
    for (let day = heatStart; day <= model.today; day = mikuMidnight(day, 1)) {
      const item = model.days.get(mikuDayKey(day)), total = item ? item.linux + item.windows : 0;
      const square = make(calendar, 'div', `miku-day miku-level-${total ? Math.max(1, Math.ceil(4 * Math.min(1, total / heatMax))) : 0}`);
      const label = `${new Date(day).toLocaleDateString('ru-RU', { weekday: 'short', day: 'numeric', month: 'long' })}: ${mikuDuration(total)}`;
      square.title = label; square.setAttribute('aria-label', label); square.setAttribute('role', 'img'); square.tabIndex = 0;
    }
    const categoryCard = make(root, 'section', 'miku-card');
    make(categoryCard, 'h3', '', 'Время по категориям');
    make(categoryCard, 'p', 'miku-muted', 'Категории разных установок не объединяются автоматически, даже если названия совпадают.');
    if (!model.categories.length) make(categoryCard, 'p', 'miku-empty', 'В этом периоде пока нет рабочих сегментов.');
    for (const category of model.categories) {
      const row = make(categoryCard, 'div', 'miku-category');
      const label = make(row, 'div', 'miku-source-title');
      make(label, 'span', '', `${category.name} · ${sourceName(category.os)}`);
      make(label, 'strong', '', `${mikuDuration(category.workMs)} · ${Math.round(100 * category.workMs / Math.max(1, model.workMs))}%`);
      const fill = make(make(row, 'div', 'miku-track'), 'div');
      fill.style.backgroundColor = category.color; fill.style.width = `${100 * category.workMs / Math.max(1, model.workMs)}%`;
    }
    const footer = make(root, 'footer', 'miku-footnote');
    make(footer, 'p', '', `Часовой пояс: ${Intl.DateTimeFormat().resolvedOptions().timeZone}. Дни разделяются по местной полуночи. Закрытое приложение и время после снимка не начисляются.`);
    make(footer, 'p', '', 'Сумма независимых источников, не синхронизация таймеров. При одновременной работе на двух ОС их сегменты суммируются. Копии одной истории на разных ОС суммировать нельзя.');
    for (const status of statuses.filter(item => item.error)) make(footer, 'p', 'miku-warning', `${sourceName(status.os)}: ${status.error}`);
  }

  async function reload() {
    if (disposed) return;
    if (loading) { pending = true; return; }
    loading = true;
    try {
      do {
        pending = false;
        const loaded = await Promise.all(definitions.map(async definition => {
          try {
            const text = await dv.io.load(definition.path);
            if (text == null) return { os: definition.os, error: 'Файл выгрузки пока не найден' };
            return { os: definition.os, snapshot: mikuSnapshot(JSON.parse(text), definition.os) };
          } catch (error) { return { os: definition.os, error: `Не удалось прочитать выгрузку: ${error.message}` }; }
        }));
        if (disposed) return;
        snapshots = loaded.flatMap(item => item.snapshot ? [item.snapshot] : []); statuses = loaded;
        render();
      } while (pending && !disposed);
    } finally { loading = false; }
  }
  const paths = new Set(definitions.map(source => source.path));
  const onChange = file => { if (paths.has(file.path)) void reload(); };
  const subscriptions = ['modify', 'create', 'delete'].map(event => dv.app.vault.on(event, onChange));
  subscriptions.push(dv.app.vault.on('rename', (file, old) => { if (paths.has(old) || paths.has(file.path)) void reload(); }));
  dv.container[stateKey] = { dispose: () => { disposed = true; for (const event of subscriptions) dv.app.vault.offref(event); root.remove(); } };
  await reload();
}

if (typeof dv !== 'undefined') await mikuDashboard(dv, typeof input === 'undefined' ? undefined : input);
