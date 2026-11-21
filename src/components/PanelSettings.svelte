<script lang="ts">
  import type { Snapshot, SettingsPatch } from '../lib/types';
  let { snapshot, save, choiceSound }: { snapshot: Snapshot; save: (patch: SettingsPatch) => Promise<void>; choiceSound: () => void } = $props();
  let expanded = $state(false);
  const windows = $derived(snapshot.platform === 'windows');
  function change(value: string) { void save({ backgroundMode: value as 'Companion' | 'Panel' }).catch(() => {}); }
</script>

<section class="panel-settings" aria-label="Таймер вместо помощника">
  <h3>Таймер вместо помощника</h3>
  <label>Режим сворачивания<select aria-label="Режим сворачивания" value={snapshot.settings.backgroundMode} onchange={e => change(e.currentTarget.value)}>
    <option value="Companion">Окно помощника</option>
    <option value="Panel">{windows ? 'Таймер в области уведомлений' : 'Таймер в панели Линукса'}</option>
  </select></label>
  <p class="muted">Настройка определяет, куда сворачивать главное окно кнопкой вверху, крестиком и через меню значка. Сам таймер и история продолжают работать независимо от окон.</p>
  {#if snapshot.settings.backgroundMode === 'Panel'}
    {#if windows}
      <p class="info-note">В значке области уведомлений минуты показаны сверху, секунды снизу; полное время и состояние — в подсказке и меню. Свободный текст в панели задач Windows без отдельного расширения оболочки недоступен. Открыть главное окно можно через меню значка.</p>
    {:else}
      <p class="info-note">Таймер передаёт данные напрямую из приложения в модуль Waybar, без чтения истории с диска и отдельного отсчёта. Нажатие на таймер открывает главное окно. Модуль нужно добавить в конфигурацию панели один раз — приложение её не меняет.</p>
      <button class="gloss" aria-expanded={expanded} onclick={() => { choiceSound(); expanded = !expanded; }}>Настройка модуля панели</button>
      {#if expanded}
        <p class="muted">Добавьте «custom/miku-focus» в существующий список «modules-right» или «modules-left», а блок ниже — в корень конфигурации Waybar. Затем перечитайте конфигурацию панели. Для копирования выделите текст и нажмите Ctrl+C.</p>
        <textarea aria-label="Конфигурация модуля панели" readonly value={snapshot.panel.waybarConfig} spellcheck={false}></textarea>
      {/if}
    {/if}
    <p class="info-note">После сворачивания оба окна скрыты. Голос и звуки скрытых окон не воспроизводятся: завершение видно по состоянию таймера в панели, а вопрос о продлении появится при открытии большого окна. При выходе из полностью скрытого режима главное окно кратко показывается для обычного затемнения и звука выхода.</p>
  {/if}
  {#if snapshot.panel.error}<p class="error" role="alert">{snapshot.panel.error}. Для сворачивания пока используется окно помощника.</p>{/if}
</section>

<style>
  .panel-settings { margin: 18px 0; padding-bottom: 16px; border-bottom: 1px solid #8dc9bd; }
  h3 { margin: 0 0 10px; }
  p { line-height: 1.5; }
  textarea { width: 100%; min-height: 250px; margin-top: 8px; font-family: monospace; }
  .error { color: #8b3030; }
</style>
