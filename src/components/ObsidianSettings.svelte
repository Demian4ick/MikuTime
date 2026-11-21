<script lang="ts">
  import type { ObsidianExportStatus } from '../lib/types';
  let { status, choose, disconnect, refresh }: {
    status: ObsidianExportStatus; choose: () => Promise<void>;
    disconnect: () => Promise<void>; refresh: () => Promise<void>;
  } = $props();
  let busy = $state(false);
  async function run(action: () => Promise<void>) {
    if (busy) return;
    busy = true;
    try { await action(); } catch { /* App reports command errors. */ }
    finally { busy = false; }
  }
</script>

<section class="export-panel" aria-label="Автовыгрузка для Обсидиана">
  <h3>Хранилище для Обсидиана</h3>
  <p class="muted">Выберите папку хранилища. Приложение будет обновлять в ней JSON с полной историей занятий, рабочими сегментами, перерывами и категориями. Настройки и личные пути к ресурсам не выгружаются.</p>
  <div class="inline-actions">
    <button class="gloss" disabled={busy} onclick={() => run(choose)}>{status.folder ? 'Изменить папку хранилища' : 'Выбрать папку хранилища'}</button>
    {#if status.folder}
      <button class="gloss" disabled={busy || status.exporting} onclick={() => run(refresh)}>Обновить выгрузку</button>
      <button class="text-button" disabled={busy} onclick={() => run(disconnect)}>Отключить автовыгрузку</button>
    {/if}
  </div>
  {#if status.folder}
    <dl class="system-info"><dt>Папка</dt><dd class="path">{status.folder}</dd><dt>Файл этой ОС</dt><dd class="path">{status.file}</dd></dl>
    <p role="status">{status.exporting ? 'Обновление выгрузки…' : status.error ? 'Выгрузка ожидает повторной попытки.' : status.lastExportedAt ? `Последняя выгрузка: ${new Date(status.lastExportedAt).toLocaleString('ru-RU')}` : 'Ожидание первой выгрузки.'}</p>
  {:else}
    <p class="muted" role="status">Автовыгрузка отключена. Данные хранятся только в приложении.</p>
  {/if}
  {#if status.error}<p class="export-error" role="alert">{status.error}</p>{/if}
  <p class="info-note">В обеих ОС выберите то же хранилище: файлы «linux.json» и «windows.json» независимы, их можно объединить в отчёте Обсидиана. Это выгрузка, не перенос истории между приложениями. Файлы обновляются после изменений и примерно раз в 30 секунд во время работы или отдыха. Ручные правки выгрузки могут быть заменены — заметки храните отдельно. Отключение не удаляет созданные файлы.</p>
</section>

<style>
  .export-panel { margin-top: 22px; padding-top: 16px; border-top: 1px solid #8dc9bd; }
  h3 { margin: 0 0 8px; }
  p { line-height: 1.5; }
  .path { overflow-wrap: anywhere; }
  .export-error { color: #8b3030; }
</style>
