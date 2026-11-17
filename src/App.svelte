<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { invoke, convertFileSrc } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import type { Snapshot, SettingsPatch, Category, Emotion, DialogueFile } from './lib/types';
  import { AutoSaver, type SaveStatus } from './lib/autosave';
  import { EmotionManager } from './lib/emotions';
  import { emotionNames } from './lib/localization';
  import { ReactionController } from './lib/reactions';
  import type { VisualState } from './lib/reactions';
  import { validateDialogues } from './lib/dialogues';
  import { SfxManager, SoundPreview, type SoundKind } from './lib/sounds';
  import { hoverButton, closePresentation, exitFadeMs, exitAudioLimitMs } from './lib/interaction';
  import { StatisticsService, formatDuration, formatTimer, dayKey } from './lib/statistics';
  import TimerControls from './components/TimerControls.svelte';
  import Diary from './components/Diary.svelte';
  import Settings from './components/Settings.svelte';
  import DialoguePanel from './components/DialoguePanel.svelte';

  const companion = new URLSearchParams(location.search).has('companion');
  const label = companion ? 'companion' : 'main';
  let snapshot = $state<Snapshot | null>(null);
  let page = $state<'focus' | 'diary' | 'settings'>('focus');
  let toast = $state(''); let fatal = $state('');
  let context = $state(false);
  let visual = $state<VisualState>({ emotion: 'Neutral', ru: 'Я рядом, сэмпай. Всё получится~ ♡', en: "I'm here with you, Senpai.", speaking: false, typing: false, awaitingInput: false });
  let previousSprite = $state(''); let transition = $state(false);
  let sprite = $derived(companion ? '/companion/SmartyMiku.png' : snapshot ? EmotionManager.path(visual.emotion, snapshot.settings, convertFileSrc) : '/sprites/Neutral.png');
  let background = $derived(snapshot?.settings.background ? convertFileSrc(snapshot.settings.background) : '/background.jpg');
  let isActive = $derived(snapshot?.activeWindow === label);
  let controller: ReactionController | null = null;
  let dialogueData: DialogueFile | null = null;
  let dialogueEditor = $state(false); let dialogueText = $state('');
  let pointerStart: { x: number; y: number } | null = null; let dragged = false;
  let dragReady: Promise<unknown> = Promise.resolve(); let dragBusy = false; let lastDrag = 0;
  let transitionTimeout: ReturnType<typeof setTimeout> | undefined;
  let toastTimeout: ReturnType<typeof setTimeout> | undefined;
  let goalRequestedDay = '';
  let saveStatus = $state<SaveStatus>('idle');
  const settingsSaver = new AutoSaver<SettingsPatch>(patch => call('update_settings', { patch }), value => saveStatus = value);
  const categorySavers = new Map<string, AutoSaver<Category>>();
  let durationWrites: Promise<void> = Promise.resolve();
  let soundWrites: Promise<void> = Promise.resolve();
  const effects = new SfxManager(() => snapshot?.settings ?? null, convertFileSrc, notify);
  const soundPreview = new SoundPreview(notify);
  let previewGeneration = 0;
  let resizeReady: Promise<unknown> = Promise.resolve(); let resizing = false; let resizeBusy = false; let lastResize = 0;
  let geometrySaveTimeout: ReturnType<typeof setTimeout> | undefined;
  let closing = $state(false), fading = $state(false);

  function notify(text: string) { toast = text; clearTimeout(toastTimeout); toastTimeout = setTimeout(() => toast = '', 12000); }
  async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    try { return await invoke<T>(command, args); } catch (e) { notify(String(e)); throw e; }
  }
  function act(action: string) {
    void (async () => {
      if (action === 'start') await Promise.all([settingsSaver.flush(), durationWrites]);
      await call('timer_action', { action });
    })().catch(() => {});
    setContextMenu(false);
  }
  function windowAction(action: string) {
    void (async () => {
      await soundWrites;
      if (action === 'quit') await Promise.all([settingsSaver.flush(), durationWrites, resizeReady, ...[...categorySavers.values()].map(saver => saver.flush())]);
      await call('window_action', { action });
    })().catch(() => {});
    context = false;
  }
  function requestReaction(event: string) { void call('request_reaction', { event }).catch(() => {}); }
  function setVisual(value: VisualState) {
    if (!companion && value.emotion !== visual.emotion && snapshot) {
      previousSprite = EmotionManager.path(visual.emotion, snapshot.settings, convertFileSrc); transition = true;
      clearTimeout(transitionTimeout); transitionTimeout = setTimeout(() => transition = false, 240);
    }
    visual = value;
  }
  function syncController() {
    if (!snapshot || snapshot.activeWindow !== label || !dialogueData || closing) { controller?.dispose(); controller = null; return; }
    const restoreDecision = !controller && snapshot.timer.status === 'AwaitingDecision' && snapshot.timer.phase === 'Focus';
    if (!controller) controller = new ReactionController(dialogueData, () => snapshot!.settings, convertFileSrc, setVisual, notify);
    controller.setContext(snapshot.timer.phase !== 'Focus');
    controller.setDecision(snapshot.timer.status === 'AwaitingDecision');
    controller.configure();
    // The companion may already have claimed the event; restore from the Rust decision state.
    if (restoreDecision) controller.event('focus_complete');
    for (const event of snapshot.reactions) {
      void invoke<boolean>('claim_reaction', { id: event.id }).then(claimed => {
        if (claimed && snapshot?.activeWindow === label && !document.hidden && !closing) {
          stopPreview(); playEffect(event.event);
          if (!(restoreDecision && event.event === 'focus_complete')) controller?.event(event.event);
        }
      }).catch(e => notify(String(e)));
    }
  }
  function update(value: Snapshot) {
    const previous = snapshot;
    snapshot = value;
    effects.configure(value.timer.phase !== 'Focus'); soundPreview.configure(value.settings);
    if (value.activeWindow !== label) { stopPreview(); effects.stop(); }
    else if (previous && previous.activeWindow !== label) {
      // Play in the newly active WebView: the old one is already hidden.
      playEffect(companion ? 'main_close' : 'main_open');
      if (!companion && value.timer.status === 'AwaitingDecision') page = 'focus';
    }
    if (previous && previous.timer.phase !== value.timer.phase) stopPreview();
    if (value.quitting && value.activeWindow === label) void presentQuit();
    else if (!value.quitting) { closing = false; fading = false; }
    syncController();
  }
  function playEffect(event: string) { if (!closing && snapshot?.activeWindow === label && !document.hidden) void effects.event(event).catch(e => notify(String(e))); }
  function navigate(next: typeof page) {
    if (closing) return;
    playEffect('menu_open');
    if (page !== next) {
      stopPreview();
      page = next;
    }
  }
  function setContextMenu(open: boolean) { if (context !== open) { playEffect(open ? 'menu_open' : 'menu_close'); context = open; } }
  function advanceDialogue() { playEffect('dialogue_continue'); controller?.advance(); }
  function hover(event: PointerEvent) {
    if (event.target instanceof Element && hoverButton(event.target, event.relatedTarget instanceof Node ? event.relatedTarget : null)) playEffect('choice_hover');
  }
  function visibilityChanged() { if (document.hidden && !closing) { stopPreview(); effects.stop(); controller?.silence(); } }
  async function presentQuit() {
    if (closing) return;
    closing = true; context = false; dialogueEditor = false;
    controller?.dispose(); controller = null; stopPreview(); effects.stop();
    try {
      await closePresentation(
        async () => { await Promise.all([soundWrites, settingsSaver.flush(), durationWrites, resizeReady, ...[...categorySavers.values()].map(saver => saver.flush())]); },
        async () => {
          const limit = setTimeout(() => effects.stop(), exitAudioLimitMs);
          try { await effects.event('app_quit'); } finally { clearTimeout(limit); }
        },
        async () => { fading = true; await tick(); await new Promise(resolve => setTimeout(resolve, exitFadeMs)); },
        () => call<void>('complete_quit'),
      );
    } catch (error) {
      effects.stop(); closing = false; fading = false;
      await call('cancel_quit').catch(() => {}); notify(String(error));
    }
  }
  function save(patch: SettingsPatch) { return settingsSaver.save(patch); }
  function adjustFocusDuration(delta: -1 | 1) {
    // Every click is a relative Rust operation; fast clicks must not reuse a stale snapshot.
    const request = durationWrites.catch(() => {}).then(() => settingsSaver.flush()).then(async () => {
      await call<void>('adjust_focus_minutes', { delta });
      playEffect(delta > 0 ? 'duration_up' : 'duration_down');
    });
    durationWrites = request;
    return request;
  }
  function categorySave(category: Category) {
    if (!categorySavers.has(category.id)) categorySavers.set(category.id, new AutoSaver(value => call('save_category', { category: value })));
    return categorySavers.get(category.id)!.save(category);
  }
  function queueSound(write: () => Promise<void>) {
    const request = soundWrites.then(write);
    soundWrites = request.catch(() => {});
    return request;
  }
  function bindSfx(event: string, sound: string | null) { return queueSound(() => call<void>('bind_sfx', { event, sound })); }
  function clearSound(kind: SoundKind, id: string) { return queueSound(() => call<void>('clear_sound', { kind, id })); }
  function stopPreview() { previewGeneration++; soundPreview.stop(); }
  async function preview(kind: SoundKind, id: string) {
    const token = ++previewGeneration;
    await Promise.all([settingsSaver.flush(), soundWrites]);
    if (token !== previewGeneration || !snapshot || snapshot.activeWindow !== label || document.hidden) return;
    controller?.silence(); effects.stop();
    await soundPreview.play(kind, id, snapshot.settings, convertFileSrc);
  }
  function upload(file: File, kind: string, id: string): Promise<void> {
    return kind === 'voice' || kind === 'sfx' ? queueSound(() => importFile(file, kind, id)) : importFile(file, kind, id);
  }
  async function importFile(file: File, kind: string, id: string) {
    try {
      if (file.size > 20 * 1024 * 1024) throw new Error('Файл больше 20 МиБ.');
      if (kind === 'background' || kind === 'sprite') {
        const bitmap = await createImageBitmap(file);
        const valid = bitmap.width <= 4096 && bitmap.height <= 4096;
        bitmap.close();
        if (!valid) throw new Error('Изображение должно быть не больше 4096×4096.');
      }
      // Raw binary IPC avoids a huge JSON number[] and duplicate resource copies in RAM.
      await invoke('import_asset', await file.arrayBuffer(), { headers: { 'x-miku-kind': kind, 'x-miku-id': id } });
      notify(`Загружено: ${file.name}`);
    } catch (e) { notify(String(e)); throw e; }
  }
  async function exportData(format: string) { try { notify(`Экспорт сохранён: ${await call<string>('export_data', { format })}`); } catch {} }
  async function editDialogues() {
    try { dialogueText = JSON.stringify(await call('get_dialogues'), null, 2); dialogueEditor = true; } catch {}
  }
  async function saveDialogues() {
    try {
      const data = validateDialogues(JSON.parse(dialogueText)); await call('save_dialogues', { data });
      dialogueData = data; controller?.replace(data); dialogueEditor = false; notify('JSON диалогов сохранён.');
    } catch (e) { notify(String(e)); }
  }
  function pointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    pointerStart = { x: event.clientX, y: event.clientY }; dragged = false;
    if (snapshot?.platform.startsWith('Hyprland')) (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }
  function pointerMove(event: PointerEvent) {
    if (!pointerStart || !event.buttons) return;
    if (snapshot?.platform.startsWith('Hyprland')) {
      if (!dragged && Math.hypot(event.clientX - pointerStart.x, event.clientY - pointerStart.y) > 6) {
        dragged = true; dragReady = call('drag_companion', { phase: 'start' }).catch(() => {});
      }
      if (dragged && !dragBusy && performance.now() - lastDrag >= 32) {
        dragBusy = true; lastDrag = performance.now();
        void dragReady.then(() => call('drag_companion', { phase: 'move' })).catch(() => {}).finally(() => dragBusy = false);
      }
      return;
    }
    if (dragged) return;
    if (Math.hypot(event.clientX - pointerStart.x, event.clientY - pointerStart.y) > 6) {
      dragged = true; pointerStart = null;
      void getCurrentWindow().startDragging().catch(e => notify(`Перетаскивание: ${e}`));
    }
  }
  function pointerUp() {
    if (dragged && snapshot?.platform.startsWith('Hyprland')) void dragReady.then(() => call('drag_companion', { phase: 'end' })).catch(() => {});
    pointerStart = null;
  }
  function companionClick(event: MouseEvent) {
    pointerStart = null;
    // Keyboard/accessibility activation must not inherit a previous pointer drag.
    if (!dragged || event.detail === 0) windowAction('main');
  }
  function saveGeometrySoon() {
    if (!companion || !isActive) return;
    clearTimeout(geometrySaveTimeout);
    geometrySaveTimeout = setTimeout(() => {
      if (isActive) void call('save_companion_geometry').catch(() => {});
    }, 300);
  }
  function resizeDown(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault(); event.stopPropagation();
    if (snapshot?.platform.startsWith('Hyprland')) {
      (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
      resizing = true;
      resizeReady = call('resize_companion', { phase: 'start' }).catch(() => {});
    } else {
      void getCurrentWindow().startResizeDragging('SouthEast').catch(e => notify(`Размер помощника: ${e}`));
    }
  }
  function resizeMove(event: PointerEvent) {
    if (!resizing || !event.buttons || resizeBusy || performance.now() - lastResize < 32) return;
    resizeBusy = true; lastResize = performance.now();
    resizeReady = resizeReady.then(() => call('resize_companion', { phase: 'move' })).catch(() => {}).finally(() => resizeBusy = false);
  }
  function resizeEnd() {
    if (!resizing) return;
    resizing = false;
    resizeReady = resizeReady.then(() => call('resize_companion', { phase: 'end' })).catch(() => {});
  }
  function resizeKey(event: KeyboardEvent) {
    const delta: Record<string, [number, number]> = { ArrowLeft: [-10, 0], ArrowRight: [10, 0], ArrowUp: [0, -10], ArrowDown: [0, 10] };
    const step = delta[event.key]; if (!step) return;
    event.preventDefault(); event.stopPropagation();
    void call('adjust_companion_size', { deltaWidth: step[0], deltaHeight: step[1] }).catch(() => {});
  }

  onMount(() => {
    const unlisteners: (() => void)[] = []; let disposed = false;
    void (async () => {
      try {
        const unstate = await listen<Snapshot>('state', e => { if (!disposed) update(e.payload); });
        if (disposed) { unstate(); return; } unlisteners.push(unstate);
        const unclock = await listen<{ now: number; remainingMs: number; timer: Snapshot['timer'] }>('clock', e => {
          if (!snapshot || disposed || snapshot.activeWindow !== label) return;
          snapshot = { ...snapshot, ...e.payload };
          if (snapshot.settings.dailyGoalMinutes > 0 && goalRequestedDay !== dayKey(snapshot.now)) {
            const today = new StatisticsService(snapshot).get(dayKey(snapshot.now));
            if (today.workMs >= snapshot.settings.dailyGoalMinutes * 60_000) { goalRequestedDay = dayKey(snapshot.now); requestReaction('goal'); }
          }
        });
        if (disposed) { unclock(); return; } unlisteners.push(unclock);
        dialogueData = validateDialogues(await invoke('get_dialogues'));
        if (!disposed) update(await invoke<Snapshot>('get_state'));
      } catch (e) { fatal = String(e); }
    })();
    return () => { disposed = true; for (const unlisten of unlisteners) unlisten(); controller?.dispose(); stopPreview(); effects.stop(); clearTimeout(transitionTimeout); clearTimeout(toastTimeout); clearTimeout(geometrySaveTimeout); };
  });
</script>

<svelte:document onvisibilitychange={visibilityChanged}/>
<svelte:window onpointerover={hover} onresize={saveGeometrySoon} onkeydown={e => { if (e.key === 'Escape') { setContextMenu(false); dialogueEditor = false; stopPreview(); controller?.skip(); } }}/>

{#if companion}
  <main class="companion" inert={closing} class:inactive={!isActive} oncontextmenu={e => { e.preventDefault(); setContextMenu(!context); }}>
    <button class="companion-character" data-hover-exclude aria-label="Мику: клик — открыть, перетаскивание — переместить" onclick={companionClick} onpointerdown={pointerDown} onpointermove={pointerMove} onpointerup={pointerUp} onpointercancel={pointerUp}>
      {#if transition}<img class="sprite-old" src={previousSprite} alt="" draggable="false"/>{/if}
      <img class="sprite-current" src={sprite} alt="Мику" draggable="false"/>
    </button>
    {#if snapshot}<button class="companion-clock" onclick={() => windowAction('main')} title="Открыть Miku Focus"><span class:working={snapshot.timer.status === 'Focus'}>♡</span> {formatTimer(snapshot.remainingMs)} <small>{snapshot.timer.phase !== 'Focus' ? 'отдых' : snapshot.timer.status === 'Paused' ? 'Ⅱ' : snapshot.timer.status === 'AwaitingDecision' ? '+5?' : '✧'}</small></button>{/if}
    {#if visual.speaking && isActive}<div class="companion-bubble" data-hover-exclude><p id="companion-dialogue-text">{visual.ru}</p>{#if visual.awaitingInput}<small>Нажмите, чтобы продолжить ▸</small>{/if}<button class="bubble-advance" aria-label="Продолжить диалог" aria-describedby="companion-dialogue-text" onclick={advanceDialogue}></button></div>{/if}
    <button class="resize-handle" data-hover-exclude aria-label="Изменить размер помощника" title="Потяните угол мышкой. Клавиши стрелок меняют размер на 10 пикселей."
      onpointerdown={resizeDown} onpointermove={resizeMove} onpointerup={resizeEnd} onpointercancel={resizeEnd} onkeydown={resizeKey}>
      <svg viewBox="0 0 18 18" aria-hidden="true"><path d="M4 14 14 4M9 14l5-5"/></svg>
    </button>
    {#if context && snapshot}<div class="companion-menu"><b>Miku Focus</b><button onclick={() => windowAction('main')}>Открыть главное окно</button>{#if snapshot.timer.phase !== 'Focus'}<button onclick={() => act('skip')}>Закончить отдых</button>{:else if snapshot.timer.status === 'Focus'}<button onclick={() => act('pause')}>Пауза</button>{:else if snapshot.timer.status === 'Paused'}<button onclick={() => act('resume')}>Продолжить работу</button>{:else if snapshot.timer.status === 'AwaitingDecision'}<button onclick={() => act('extend')}>Ещё 5 минут ♡</button><button onclick={() => act('finish')}>Завершить и отдохнуть</button>{:else}<button onclick={() => act('start')}>Начать работу</button>{/if}<button onclick={() => { void save({ alwaysOnTop: !snapshot!.settings.alwaysOnTop }).catch(() => {}); context = false; }}>{snapshot.settings.alwaysOnTop ? '✓ ' : ''}Поверх окон</button><button onclick={() => { controller?.skip(); context = false; }}>Пропустить реплику</button><button onclick={() => windowAction('quit')}>Сохранить и выйти</button></div>{/if}
    {#if toast || fatal}<div class="companion-error" role="alert">{toast || fatal}</div>{/if}
  </main>
{:else}
  <main class="app-shell" inert={closing}>
    <header class="app-header"><a class="brand" href="#focus" onclick={() => navigate('focus')}><span class="brand-flower">✿</span><span>Miku <b>Focus</b></span></a><div class="header-actions"><button class="header-button" class:muted={snapshot?.settings.quietMode} title="Тихий режим" aria-label="Переключить тихий режим" onclick={() => { if (snapshot) void save({ quietMode: !snapshot.settings.quietMode }).catch(() => {}); }}>{snapshot?.settings.quietMode ? '♪̸' : '♫'}</button><button class="gloss small" onclick={() => windowAction('companion')}>Помощник <span>↗</span></button><button class="header-button" aria-label="Сохранить и выйти" title="Сохранить и выйти" onclick={() => windowAction('quit')}>⏻</button></div></header>
    <nav class="main-tabs"><button class:active={page === 'focus'} onclick={() => navigate('focus')}>♡ Комната фокуса</button><button class:active={page === 'diary'} onclick={() => navigate('diary')}>✧ Дневник занятий</button><button class:active={page === 'settings'} onclick={() => navigate('settings')}>Настройки</button><span class="nav-date">{new Date(snapshot?.now ?? Date.now()).toLocaleDateString('ru-RU', { day: 'numeric', month: 'long' })}</span></nav>
    {#if fatal}<section class="fatal-error"><h2>Не удалось открыть локальные данные</h2><p>{fatal}</p><p>Файлы не удалены. Для запуска используйте <code>npm run tauri dev</code>, не браузер.</p></section>
    {:else if !snapshot}<div class="loading">Мику уже идёт… ♡</div>
    {:else if page === 'diary' && isActive}<Diary {snapshot} {exportData}/>
    {:else if page === 'settings' && isActive}<Settings {snapshot} {save} status={saveStatus} {categorySave} {upload} resetArt={() => { void call('reset_art').catch(() => {}); }} {editDialogues} {bindSfx} {clearSound} {preview} {stopPreview}/>
    {:else if page === 'focus'}
      <section class="focus-scene" style:background-image={`linear-gradient(90deg,rgba(224,255,247,.18),rgba(243,255,251,.12) 62%,rgba(236,255,249,.85)),url("${background}")`}>
        <div class="scene-decoration heart-one">♡</div><div class="scene-decoration heart-two">✧</div>
        <button class="main-character" data-hover-exclude onclick={() => requestReaction('touch')} aria-label="Поговорить с Мику">
          {#if transition}<img class="sprite-old" src={previousSprite} alt="" draggable="false"/>{/if}<img class="sprite-current" src={sprite} alt={`Мику: ${emotionNames[visual.emotion]}`} draggable="false"/>
        </button>
        <div class="focus-sidebar"><label class="category-picker"><span>Категория</span><select value={snapshot.selectedCategory} onchange={e => { void call('select_category', { id: e.currentTarget.value }).catch(() => {}); }}>{#each snapshot.categories.filter(c => !c.archived) as category}<option value={category.id}>{category.name}</option>{/each}</select></label><TimerControls {snapshot} {act} {adjustFocusDuration}/><div class="today-mini">Сегодня <strong>{formatDuration(new StatisticsService(snapshot).get(dayKey(snapshot.now)).workMs)}</strong></div></div>
      </section>
      <DialoguePanel {visual} advance={advanceDialogue} skip={() => controller?.skip()}/>
    {/if}
    <footer class="app-footer"><span>Miku Focus</span><span>{snapshot?.recovered ? 'Таймер восстановлен' : ''}</span></footer>
    {#if toast || snapshot?.error}<div class="toast" role="status"><span>{toast || snapshot?.error}</span><button aria-label="Закрыть уведомление" onclick={() => { toast = ''; if (snapshot) snapshot = { ...snapshot, error: null }; }}>×</button></div>{/if}
    {#if dialogueEditor}<div class="modal-backdrop"><div class="json-editor" role="dialog" tabindex="-1" aria-modal="true" aria-label="Редактор диалогов"><h2>Диалоги Мику <span>♡</span></h2><p>en — английская озвучка, ru — субтитры, emotion — ID, voice — имя WAV/OGG без расширения.</p><textarea aria-label="JSON диалогов" spellcheck="false" bind:value={dialogueText}></textarea><div><button class="gloss primary" onclick={saveDialogues}>Сохранить</button><button class="gloss" onclick={() => dialogueEditor = false}>Отмена</button></div></div></div>{/if}
  </main>
{/if}
{#if closing && isActive}<div class="shutdown-curtain" class:fading style:--exit-fade={`${exitFadeMs}ms`} role="status" aria-label="Завершение работы"></div>{/if}
