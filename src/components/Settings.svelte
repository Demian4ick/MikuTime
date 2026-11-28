<script lang="ts">
  import type { Snapshot, SettingsPatch, Category, Emotion } from '../lib/types';
  import type { SaveStatus } from '../lib/autosave';
  import NumberSetting from './NumberSetting.svelte';
  import CategoryEditor from './CategoryEditor.svelte';
  import SoundSettings from './SoundSettings.svelte';
  import ObsidianSettings from './ObsidianSettings.svelte';
  import PanelSettings from './PanelSettings.svelte';
  import AppearanceSettings from './AppearanceSettings.svelte';
  import type { SoundKind } from '../lib/sounds';
  import type { ArrowDirection } from '../lib/interaction';
  import { emotionFiles } from '../lib/emotions';
  import { emotionNames, talkativenessNames, idleStatusText } from '../lib/localization';
  let { snapshot, save, status, categorySave, categoryDelete, upload, uploadWallpapers, selectWallpaper, removeWallpaper, resetArt, editDialogues, bindSfx, clearSound, useBuiltinAudio, preview, stopPreview, arrowSound, choiceSound, chooseObsidian, disconnectObsidian, refreshObsidian }: {
    snapshot: Snapshot; save: (patch: SettingsPatch) => Promise<void>; status: SaveStatus; categorySave: (category: Category) => Promise<void>;
    categoryDelete: (id: string) => Promise<void>;
    uploadWallpapers: (files: File[]) => Promise<void>; selectWallpaper: (id: string | null) => Promise<void>; removeWallpaper: (id: string) => Promise<void>;
    upload: (file: File, kind: string, id: string) => Promise<void>; resetArt: () => void; editDialogues: () => void;
    bindSfx: (event: string, sound: string | null) => Promise<void>; clearSound: (kind: SoundKind, id: string) => Promise<void>;
    useBuiltinAudio: (id?: string) => Promise<void>;
    preview: (kind: SoundKind, id: string) => Promise<void>; stopPreview: () => void;
    arrowSound: (direction: ArrowDirection) => void;
    choiceSound: () => void;
    chooseObsidian: () => Promise<void>; disconnectObsidian: () => Promise<void>; refreshObsidian: () => Promise<void>;
  } = $props();
  let tab = $state('timer');
  let emotion = $state<Emotion>('Neutral');
  let name = $state(''); let color = $state('#4cb9b1');
  let adding = $state(false), categoryError = $state('');
  let nameMissing = $state(false);
  let categoryNameInput = $state<HTMLInputElement>();
  let categories = $derived(snapshot.categories.filter(category => !category.deleted));
  let activeCategories = $derived(categories.filter(category => !category.archived).length);
  function changeTab(next: string) { choiceSound(); tab = next; }
  function change(patch: SettingsPatch) { void save(patch).catch(() => {}); }
  async function files(event: Event, kind: string, id: string) {
    const input = event.target as HTMLInputElement;
    try { for (const file of Array.from(input.files ?? [])) await upload(file, kind, kind === 'voice' ? file.name.replace(/\.[^.]+$/, '') : id); } catch {}
    input.value = '';
  }
  async function addCategory() {
    if (adding) return;
    if (!name.trim()) {
      nameMissing = true; categoryError = 'Сначала придумайте название категории.';
      categoryNameInput?.focus();
      return;
    }
    nameMissing = false;
    adding = true; categoryError = '';
    try { await categorySave({ id: '', name: name.trim(), color, archived: false }); name = ''; }
    catch (error) { categoryError = String(error); }
    finally { adding = false; }
  }
</script>

<section class="settings scroll-area">
  <div class="section-heading"><h2>Настройки</h2><span class="muted small-text" role="status">{status === 'saving' ? 'Сохранение…' : status === 'error' ? 'Ошибка сохранения' : 'Сохраняется автоматически'}</span></div>
  <div class="subtabs"><button class:active={tab === 'timer'} onclick={() => changeTab('timer')}>Таймер</button><button class:active={tab === 'miku'} onclick={() => changeTab('miku')}>Мику и голос</button><button class:active={tab === 'appearance'} onclick={() => changeTab('appearance')}>Оформление</button><button class:active={tab === 'sounds'} onclick={() => changeTab('sounds')}>Звуки</button><button class:active={tab === 'categories'} onclick={() => changeTab('categories')}>Категории</button><button class:active={tab === 'system'} onclick={() => changeTab('system')}>Система</button></div>
  {#if tab === 'timer'}
    <div class="settings-grid">
      <NumberSetting {arrowSound} value={snapshot.settings.focusMinutes} label="Работа, мин" min={1} max={480} save={value => save({ focusMinutes: value })}/>
      <NumberSetting {arrowSound} value={snapshot.settings.shortBreakMinutes} label="Короткий отдых, мин" min={1} max={180} save={value => save({ shortBreakMinutes: value })}/>
      <NumberSetting {arrowSound} value={snapshot.settings.longBreakMinutes} label="Длинный отдых, мин" min={1} max={180} save={value => save({ longBreakMinutes: value })}/>
      <NumberSetting {arrowSound} value={snapshot.settings.sessionsBeforeLong} label="Сессий перед длинным отдыхом" min={1} max={20} save={value => save({ sessionsBeforeLong: value })}/>
      <NumberSetting {arrowSound} value={snapshot.settings.dailyGoalMinutes} label="Дневная цель, мин (0 — выключить)" min={0} max={1440} save={value => save({ dailyGoalMinutes: value })}/>
    </div>
    <label class="check-row"><input type="checkbox" checked={snapshot.settings.autoStart} onchange={e => change({ autoStart: e.currentTarget.checked })}/> Автоматически начинать работу после отдыха</label>
    <p class="info-note">После работы Мику дождётся твоего решения. При выборе отдыха он начинается сразу, без запуска и паузы. Продление — ещё 5 минут в той же рабочей сессии. Изменение длительности не прерывает текущий таймер.</p>
  {:else if tab === 'miku'}
    <div class="settings-grid">
      <NumberSetting {arrowSound} value={snapshot.settings.volume} label="Громкость" type="range" min={0} max={1} step={0.05} save={value => save({ volume: value })}/>
      <label>Разговорчивость<select value={snapshot.settings.talkativeness} onchange={e => change({ talkativeness: e.currentTarget.value as Snapshot['settings']['talkativeness'] })}>{#each Object.entries(talkativenessNames) as [id, name]}<option value={id}>{name}</option>{/each}</select></label>
      <div><NumberSetting {arrowSound} value={snapshot.settings.textSpeed} label="Скорость появления символов, симв/с" min={0} max={200} save={value => save({ textSpeed: value })}/><small class="muted">0 — показывать текст сразу.</small></div>
    </div>
    <label class="check-row"><input type="checkbox" checked={snapshot.settings.quietMode} onchange={e => change({ quietMode: e.currentTarget.checked })}/> Тихий режим — без голосов и эффектов, с субтитрами</label>
    <div class="asset-row"><div><b>Обои и тема</b><small>Своя галерея фонов и тёмное оформление.</small></div><button class="gloss" onclick={() => changeTab('appearance')}>Открыть оформление</button></div>
    <div class="asset-row"><div><b>Заменить спрайт</b><select aria-label="Эмоция" bind:value={emotion}>{#each Object.keys(emotionFiles) as id}<option value={id}>{emotionNames[id as Emotion]}</option>{/each}</select></div><label class="gloss file-button">Загрузить PNG<input aria-label="Загрузить спрайт" type="file" accept="image/png" onchange={e => files(e, 'sprite', emotion)}/></label></div>
    <div class="asset-row"><div><b>Голоса и звуковые эффекты</b><small>Таблицы файлов, прослушивание, замена и назначения — в отдельном разделе.</small></div><button class="gloss" onclick={() => changeTab('sounds')}>Открыть таблицу звуков</button></div>
    <div class="inline-actions"><button class="gloss" onclick={editDialogues}>Редактировать JSON диалогов</button><button class="text-button" onclick={resetArt}>Вернуть исходные изображения</button></div>
  {:else if tab === 'appearance'}
    <AppearanceSettings settings={snapshot.settings} {save} upload={uploadWallpapers} select={selectWallpaper} remove={removeWallpaper} {choiceSound}/>
  {:else if tab === 'sounds'}
    <SoundSettings settings={snapshot.settings} {save} {upload} {bindSfx} {clearSound} {useBuiltinAudio} {preview} {stopPreview} {arrowSound} {choiceSound}/>
  {:else if tab === 'categories'}
    <h3>Создать категорию</h3>
    <form class="category-editor" class:needs-name={nameMissing} novalidate onsubmit={e => { e.preventDefault(); void addCategory(); }}>
      <input type="color" bind:value={color} disabled={adding} aria-label="Цвет новой категории"/>
      <input bind:this={categoryNameInput} placeholder={nameMissing ? 'Придумайте название категории' : 'Название новой категории…'} value={name} oninput={e => { name = e.currentTarget.value; if (nameMissing && name.trim()) { nameMissing = false; categoryError = ''; } }} maxlength="60" required disabled={adding} aria-label="Новая категория" aria-invalid={nameMissing} aria-describedby="category-name-hint"/>
      <button class="gloss" type="submit" disabled={adding || categories.length >= 200}>{adding ? 'Создание…' : '+ Создать'}</button>
    </form>
    {#if categoryError}<p id="category-name-hint" class="category-name-hint" class:missing={nameMissing} role="alert">{categoryError}</p>
    {:else}<p id="category-name-hint" class="muted small-text">Например: Учёба или Рисование.</p>{/if}
    {#if categories.length >= 200}<p class="muted">Максимум 200 категорий. Удалите ненужную, чтобы создать новую.</p>{/if}
    <h3>Мои категории</h3>
    <p class="muted">Удаление не стирает занятия из дневника. Для работы нужна хотя бы одна активная категория.</p>
    {#each categories as category (category.id)}
      <CategoryEditor {category} save={categorySave} remove={categoryDelete} lastActive={!category.archived && activeCategories === 1}/>
    {/each}
  {:else}
    <PanelSettings {snapshot} {save} {choiceSound}/>
    <label class="check-row"><input type="checkbox" checked={snapshot.settings.alwaysOnTop} onchange={e => change({ alwaysOnTop: e.currentTarget.checked })}/> Компактный помощник поверх других окон</label>
    <dl class="system-info"><dt>Платформа</dt><dd>{snapshot.platform}</dd><dt>Отрисовка</dt><dd>{snapshot.rendererCompatibility ? 'Режим совместимости (только для приложения)' : 'Обычный режим'}</dd><dt>Бездействие системы</dt><dd>{idleStatusText(snapshot.idleAdapter)}{snapshot.systemIdle ? ' · нет ввода более 5 минут' : ''}</dd><dt>Локальные данные</dt><dd class="path">{snapshot.storagePath}</dd><dt>Восстановление</dt><dd>Работа — на паузе с контрольной точки (до 30 секунд). Отдых продолжается сам. Время закрытого приложения не учитывается.</dd></dl>
    <p class="info-note">Wayland не гарантирует позиционирование и показ поверх окон. В Hyprland адаптер управляет только окнами Miku Focus. Конфигурация оконного менеджера не изменяется. Клик — открыть, перетаскивание — за Мику, правый клик — меню. Прозрачные области окна пока принимают ввод.</p>
    <p class="muted">Отсутствие ввода не означает безделье. Таймер никогда не останавливается автоматически из-за бездействия.</p>
    <ObsidianSettings status={snapshot.obsidian} choose={chooseObsidian} disconnect={disconnectObsidian} refresh={refreshObsidian}/>
  {/if}
</section>

<style>
  .category-name-hint { font-size: 12px; color: #98455c; }
  .needs-name input:not([type=color]), .needs-name button { border-color: #ba6f89; animation: name-hint-pulse 1s ease-in-out 2; }
  .needs-name input::placeholder { color: #98455c; opacity: 1; }
  .category-name-hint.missing { font-weight: 600; }
  @keyframes name-hint-pulse { 50% { box-shadow: 0 0 0 3px #cf8da644; } }
  @media (prefers-reduced-motion: reduce) { .needs-name input:not([type=color]), .needs-name button { animation: none; } }
</style>
