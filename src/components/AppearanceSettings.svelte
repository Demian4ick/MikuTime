<script lang="ts">
  import { tick } from 'svelte';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import type { Settings, SettingsPatch } from '../lib/types';
  import { wallpaperLimit } from '../lib/appearance';
  import { builtinWallpapers } from '../lib/wallpapers';
  let { settings, save, upload, select, remove, choiceSound }: {
    settings: Settings; save: (patch: SettingsPatch) => Promise<void>;
    upload: (files: File[]) => Promise<void>; select: (id: string | null) => Promise<void>;
    remove: (id: string) => Promise<void>; choiceSound: () => void;
  } = $props();
  let busy = $state(false), error = $state(''), removing = $state<string | null>(null);
  let failed = $state(new Set<string>());
  let confirmButton = $state<HTMLButtonElement>();
  let current = $derived(settings.wallpapers.find(item => item.id === removing));
  async function run(action: () => Promise<void>) {
    if (busy) return;
    busy = true; error = '';
    try { await action(); removing = null; } catch (e) { error = String(e); }
    finally { busy = false; }
  }
  function files(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const files = Array.from(input.files ?? []); input.value = '';
    if (files.length) void run(() => upload(files));
  }
  function broken(id: string) { failed = new Set(failed).add(id); }
  async function askRemove(id: string) { error = ''; removing = id; await tick(); confirmButton?.focus(); }
  async function cancelRemove() {
    if (busy) return;
    const id = removing; removing = null; await tick();
    document.getElementById(`wallpaper-remove-${id}`)?.focus();
  }
</script>

<svelte:window onkeydown={e => { if (removing && e.key === 'Escape') { e.preventDefault(); void cancelRemove(); } }}/>
<section class="appearance-settings" aria-label="Оформление приложения">
  <h3>Тема приложения</h3>
  <div class="theme-options" role="group" aria-label="Тема приложения">
    <button class="gloss" aria-pressed={settings.theme === 'Light'} disabled={busy} onclick={() => { choiceSound(); void run(() => save({ theme: 'Light' })); }}>Светлая</button>
    <button class="gloss" aria-pressed={settings.theme === 'Dark'} disabled={busy} onclick={() => { choiceSound(); void run(() => save({ theme: 'Dark' })); }}>Тёмная</button>
  </div>
  <div class="wallpaper-heading"><h3>Галерея обоев</h3><label class="gloss file-button" class:disabled={busy || settings.wallpapers.length >= wallpaperLimit}>+ Добавить обои<input aria-label="Добавить обои" type="file" multiple accept="image/png,image/jpeg,.png,.jpg,.jpeg" disabled={busy || settings.wallpapers.length >= wallpaperLimit} onchange={files}/></label></div>
  <p class="muted small-text">PNG или JPEG, до 20 МиБ. Можно выбрать несколько файлов.</p>
  {#if error}<p class="appearance-error" role="alert">{error}</p>{/if}
  {#if busy}<p role="status" class="muted">Сохранение оформления…</p>{/if}
  {#if current}
    <div class="wallpaper-confirm" role="group" aria-label="Удаление обоев">
      <p>Убрать «{current.name}» из галереи? Файлы на диске останутся.</p>
      <div class="inline-actions"><button class="gloss small" bind:this={confirmButton} disabled={busy} onclick={() => { void run(() => remove(current!.id)); }}>Убрать</button><button class="text-button" disabled={busy} onclick={() => { void cancelRemove(); }}>Отмена</button></div>
    </div>
  {/if}
  <div class="wallpaper-grid" role="group" aria-label="Обои комнаты фокуса">
    <div class="wallpaper-card" class:selected={settings.background === null}>
      <button class="wallpaper-choice" disabled={busy} aria-label="Выбрать стандартные обои" aria-pressed={settings.background === null} onclick={() => { choiceSound(); void run(() => select(null)); }}>
        <img src="/background.jpg" alt="" loading="lazy" decoding="async"/><span>Стандартные</span>{#if settings.background === null}<small>Выбрано</small>{/if}
      </button>
    </div>
    {#each builtinWallpapers as wallpaper (wallpaper.id)}
      <div class="wallpaper-card" class:selected={settings.background === wallpaper.path}>
        <button class="wallpaper-choice" disabled={busy} aria-label={`Выбрать обои ${wallpaper.name}`} aria-pressed={settings.background === wallpaper.path} onclick={() => { choiceSound(); void run(() => select(wallpaper.id)); }}>
          <img src={wallpaper.thumbnail ?? wallpaper.path} alt="" loading="lazy" decoding="async"/>
          <span>{wallpaper.name}</span><small>{settings.background === wallpaper.path ? 'Выбрано' : 'Встроенные'}</small>
        </button>
      </div>
    {/each}
    {#each settings.wallpapers as wallpaper (wallpaper.id)}
      <div class="wallpaper-card" class:selected={settings.background === wallpaper.path}>
        <button class="wallpaper-choice" disabled={busy} aria-label={`Выбрать обои ${wallpaper.name}`} aria-pressed={settings.background === wallpaper.path} onclick={() => { choiceSound(); void run(() => select(wallpaper.id)); }}>
          {#if !failed.has(wallpaper.id)}<img src={convertFileSrc(wallpaper.thumbnail ?? wallpaper.path)} alt="" loading="lazy" decoding="async" onerror={() => broken(wallpaper.id)}/>{:else}<div class="wallpaper-missing">Нет миниатюры</div>{/if}
          <span>{wallpaper.name}</span>{#if settings.background === wallpaper.path}<small>Выбрано</small>{/if}
        </button>
        <button class="text-button" id={`wallpaper-remove-${wallpaper.id}`} disabled={busy} aria-label={`Убрать обои ${wallpaper.name}`} onclick={() => { void askRemove(wallpaper.id); }}>Убрать</button>
      </div>
    {/each}
  </div>
  {#if settings.wallpapers.length >= wallpaperLimit}<p class="muted">В галерее 100 обоев. Уберите ненужные, чтобы добавить новые.</p>{/if}
</section>

<style>
  .theme-options { display: flex; gap: 10px; margin: 12px 0 26px; }
  .theme-options button[aria-pressed=true] { outline: 2px solid var(--accent, #329b94); outline-offset: 2px; }
  .wallpaper-heading { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 10px; }
  .wallpaper-heading h3 { margin: 0; }
  .wallpaper-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 14px; align-items: start; }
  .wallpaper-card { background: var(--surface-raised, #fafff7); border: 1px solid var(--border, #a6cfc3); border-radius: 8px; overflow: hidden; }
  .wallpaper-card.selected { outline: 2px solid var(--accent, #329b94); outline-offset: 2px; }
  .wallpaper-choice { width: 100%; display: flex; flex-direction: column; align-items: stretch; padding: 0; border: 0; text-align: left; background: none; }
  .wallpaper-choice img, .wallpaper-missing { width: 100%; aspect-ratio: 16/9; object-fit: cover; }
  .wallpaper-choice span { padding: 9px 10px 5px; font-size: 12px; overflow-wrap: anywhere; }
  .wallpaper-choice small { padding: 0 10px 8px; color: var(--accent, #257b78); }
  .wallpaper-card > .text-button { margin: 0 5px 6px; }
  .wallpaper-missing { display: grid; place-items: center; font-size: 11px; color: var(--muted, #729298); }
  .appearance-error { color: var(--danger, #98455c); }
  .wallpaper-confirm { margin: 12px 0 18px; padding: 12px; background: var(--surface-raised, #fafff7); border: 1px solid var(--border, #a6cfc3); border-radius: 7px; }
  .wallpaper-confirm p { margin-bottom: 5px; }
  .disabled { opacity: .5; cursor: default; }
</style>
