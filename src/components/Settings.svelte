<script lang="ts">
  import type { Snapshot, SettingsPatch, Category, Emotion } from '../lib/types';
  import type { SaveStatus } from '../lib/autosave';
  import NumberSetting from './NumberSetting.svelte';
  import CategoryEditor from './CategoryEditor.svelte';
  import { emotionFiles } from '../lib/emotions';
  import { emotionNames, talkativenessNames, idleStatusText } from '../lib/localization';
  let { snapshot, save, status, categorySave, upload, resetArt, editDialogues }: {
    snapshot: Snapshot; save: (patch: SettingsPatch) => Promise<void>; status: SaveStatus; categorySave: (category: Category) => Promise<void>;
    upload: (file: File, kind: string, id: string) => Promise<void>; resetArt: () => void; editDialogues: () => void;
  } = $props();
  let tab = $state('timer');
  let emotion = $state<Emotion>('Neutral');
  let name = $state(''); let color = $state('#4cb9b1');
  function change(patch: SettingsPatch) { void save(patch).catch(() => {}); }
  async function files(event: Event, kind: string, id: string) {
    const input = event.target as HTMLInputElement;
    try { for (const file of Array.from(input.files ?? [])) await upload(file, kind, kind === 'voice' ? file.name.replace(/\.[^.]+$/, '') : id); } catch {}
    input.value = '';
  }
  async function addCategory() { if (!name.trim()) return; try { await categorySave({ id: '', name, color, archived: false }); name = ''; } catch {} }
</script>

<section class="settings scroll-area">
  <div class="section-heading"><h2>Настройки</h2><span class="muted small-text" role="status">{status === 'saving' ? 'Сохранение…' : status === 'error' ? 'Ошибка сохранения' : 'Сохраняется автоматически'}</span></div>
  <div class="subtabs"><button class:active={tab === 'timer'} onclick={() => tab = 'timer'}>Таймер</button><button class:active={tab === 'miku'} onclick={() => tab = 'miku'}>Мику и голос</button><button class:active={tab === 'categories'} onclick={() => tab = 'categories'}>Категории</button><button class:active={tab === 'system'} onclick={() => tab = 'system'}>Система</button></div>
  {#if tab === 'timer'}
    <div class="settings-grid">
      <NumberSetting value={snapshot.settings.focusMinutes} label="Работа, мин" min={1} max={480} save={value => save({ focusMinutes: value })}/>
      <NumberSetting value={snapshot.settings.shortBreakMinutes} label="Короткий отдых, мин" min={1} max={180} save={value => save({ shortBreakMinutes: value })}/>
      <NumberSetting value={snapshot.settings.longBreakMinutes} label="Длинный отдых, мин" min={1} max={180} save={value => save({ longBreakMinutes: value })}/>
      <NumberSetting value={snapshot.settings.sessionsBeforeLong} label="Сессий перед длинным отдыхом" min={1} max={20} save={value => save({ sessionsBeforeLong: value })}/>
      <NumberSetting value={snapshot.settings.dailyGoalMinutes} label="Дневная цель, мин (0 — выключить)" min={0} max={1440} save={value => save({ dailyGoalMinutes: value })}/>
    </div>
    <label class="check-row"><input type="checkbox" checked={snapshot.settings.autoStart} onchange={e => change({ autoStart: e.currentTarget.checked })}/> Автоматически начинать работу после отдыха</label>
    <p class="info-note">После работы Мику дождётся твоего решения. При выборе отдыха он начинается сразу, без запуска и паузы. Продление — ещё 5 минут в той же рабочей сессии. Изменение длительности не прерывает текущий таймер.</p>
  {:else if tab === 'miku'}
    <div class="settings-grid">
      <NumberSetting value={snapshot.settings.volume} label="Громкость" type="range" min={0} max={1} step={0.05} save={value => save({ volume: value })}/>
      <label>Разговорчивость<select value={snapshot.settings.talkativeness} onchange={e => change({ talkativeness: e.currentTarget.value as Snapshot['settings']['talkativeness'] })}>{#each Object.entries(talkativenessNames) as [id, name]}<option value={id}>{name}</option>{/each}</select></label>
      <div><NumberSetting value={snapshot.settings.textSpeed} label="Скорость появления символов, симв/с" min={0} max={200} save={value => save({ textSpeed: value })}/><small class="muted">0 — показывать текст сразу.</small></div>
    </div>
    <label class="check-row"><input type="checkbox" checked={snapshot.settings.quietMode} onchange={e => change({ quietMode: e.currentTarget.checked })}/> Тихий режим — без озвучки, с субтитрами</label>
    <div class="asset-row"><div><b>Твой фон</b><small>PNG/JPEG, до 20 МиБ. Оригинал не изменяется.</small></div><label class="gloss file-button">Выбрать фон<input aria-label="Загрузить фон" type="file" accept="image/png,image/jpeg" onchange={e => files(e, 'background', '')}/></label></div>
    <div class="asset-row"><div><b>Заменить спрайт</b><select aria-label="Эмоция" bind:value={emotion}>{#each Object.keys(emotionFiles) as id}<option value={id}>{emotionNames[id as Emotion]}</option>{/each}</select></div><label class="gloss file-button">Загрузить PNG<input aria-label="Загрузить спрайт" type="file" accept="image/png" onchange={e => files(e, 'sprite', emotion)}/></label></div>
    <div class="asset-row"><div><b>Озвучка персонажа</b><small>Имя файла — идентификатор озвучки в диалогах, без расширения.</small><small>Загружено: {Object.keys(snapshot.settings.voices).length}. Без аудио всё работает.</small></div><label class="gloss file-button">Загрузить звук<input aria-label="Загрузить озвучку" type="file" multiple accept=".wav,.ogg,audio/wav,audio/ogg" onchange={e => files(e, 'voice', '')}/></label></div>
    <div class="inline-actions"><button class="gloss" onclick={editDialogues}>Редактировать JSON диалогов</button><button class="text-button" onclick={resetArt}>Вернуть исходные изображения</button></div>
  {:else if tab === 'categories'}
    <p class="muted">Архивирование не удаляет историю. Категория уже запущенной сессии не меняется.</p>
    {#each snapshot.categories as category (category.id)}
      <CategoryEditor {category} save={categorySave}/>
    {/each}
    <form class="category-editor" onsubmit={e => { e.preventDefault(); void addCategory(); }}><input type="color" bind:value={color} aria-label="Цвет новой категории"/><input placeholder="Новая категория…" bind:value={name} maxlength="60" required aria-label="Новая категория"/><button class="gloss" type="submit">+ Добавить</button></form>
  {:else}
    <label class="check-row"><input type="checkbox" checked={snapshot.settings.alwaysOnTop} onchange={e => change({ alwaysOnTop: e.currentTarget.checked })}/> Компактный помощник поверх других окон</label>
    <dl class="system-info"><dt>Платформа</dt><dd>{snapshot.platform}</dd><dt>Отрисовка</dt><dd>{snapshot.rendererCompatibility ? 'Режим совместимости (только для приложения)' : 'Обычный режим'}</dd><dt>Бездействие системы</dt><dd>{idleStatusText(snapshot.idleAdapter)}{snapshot.systemIdle ? ' · нет ввода более 5 минут' : ''}</dd><dt>Локальные данные</dt><dd class="path">{snapshot.storagePath}</dd><dt>Восстановление</dt><dd>Работа — на паузе с контрольной точки (до 30 секунд). Отдых продолжается сам. Время закрытого приложения не учитывается.</dd></dl>
    <p class="info-note">Wayland не гарантирует позиционирование и показ поверх окон. В Hyprland адаптер управляет только окнами Miku Focus. Конфигурация оконного менеджера не изменяется. Клик — открыть, перетаскивание — за Мику, правый клик — меню. Прозрачные области окна пока принимают ввод.</p>
    <p class="muted">Отсутствие ввода не означает безделье. Таймер никогда не останавливается автоматически из-за бездействия.</p>
  {/if}
</section>
