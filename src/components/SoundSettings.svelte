<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import type { DialogueFile, Settings, SettingsPatch } from '../lib/types';
  import { validateDialogues } from '../lib/dialogues';
  import { effectRows, fileName, soundEvents, voiceRows, type SoundKind } from '../lib/sounds';
  import NumberSetting from './NumberSetting.svelte';
  import type { ArrowDirection } from '../lib/interaction';
  let { settings, save, upload, bindSfx, clearSound, preview, stopPreview, arrowSound, choiceSound }: {
    settings: Settings; save: (patch: SettingsPatch) => Promise<void>;
    upload: (file: File, kind: string, id: string) => Promise<void>;
    bindSfx: (event: string, sound: string | null) => Promise<void>;
    clearSound: (kind: SoundKind, id: string) => Promise<void>;
    preview: (kind: SoundKind, id: string) => Promise<void>; stopPreview: () => void;
    arrowSound: (direction: ArrowDirection) => void;
    choiceSound: () => void;
  } = $props();
  let tab = $state<'sfx' | 'voice'>('sfx');
  let query = $state(''), error = $state(''), busy = $state(false);
  let dialogues = $state<DialogueFile | null>(null);
  let playing = $state(''); let generation = 0;
  let effects = $derived(effectRows(settings));
  let voices = $derived(voiceRows(dialogues, settings));
  let search = $derived(query.trim().toLocaleLowerCase('ru-RU'));
  const matches = (...values: string[]) => !search || values.join(' ').toLocaleLowerCase('ru-RU').includes(search);
  const usage = (id: string) => soundEvents.filter(event => settings.sfxBindings?.[event.id] === id).map(event => event.name);
  function stop() { generation++; stopPreview(); playing = ''; }
  function changeTab(next: typeof tab) { stop(); choiceSound(); tab = next; }
  async function play(kind: SoundKind, id: string) {
    const key = `${kind}:${id}`;
    if (playing === key) { stop(); return; }
    stop(); const token = ++generation; playing = key; error = '';
    try { await preview(kind, id); }
    catch (e) { error = String(e); }
    finally { if (generation === token) playing = ''; }
  }
  async function files(event: Event, kind: SoundKind, replaceId?: string) {
    const input = event.currentTarget as HTMLInputElement;
    const selected = Array.from(input.files ?? []); input.value = '';
    stop(); busy = true; error = '';
    const used = new Set(kind === 'sfx' ? effects.map(row => row.id) : voices.map(row => row.id));
    try {
      for (const file of selected) {
        let id = replaceId ?? file.name.replace(/\.[^.]+$/, '');
        if (!replaceId && kind === 'sfx') {
          const base = `custom-${id.replace(/[^A-Za-z0-9_-]+/g, '-').replace(/^-+|-+$/g, '') || 'sound'}`.slice(0, 70);
          id = base; let suffix = 2; while (used.has(id)) id = `${base}-${suffix++}`;
        }
        if (!/^[A-Za-z0-9_-]{1,80}$/.test(id)) throw new Error('Для голоса используйте имя файла с латинским ID из таблицы или кнопку «Заменить» нужной реплики.');
        used.add(id); await upload(file, kind, id);
      }
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }
  async function bind(event: string, value: string) {
    stop(); error = '';
    try { await bindSfx(event, value || null); } catch (e) { error = String(e); }
  }
  async function clear(kind: SoundKind, id: string) {
    stop(); busy = true; error = '';
    try { await clearSound(kind, id); } catch (e) { error = String(e); }
    finally { busy = false; }
  }
  onMount(() => { let disposed = false;
    void invoke('get_dialogues').then(data => { if (!disposed) dialogues = validateDialogues(data); }).catch(e => { if (!disposed) error = String(e); });
    return () => { disposed = true; };
  });
  onDestroy(stop);
</script>

<div class="sound-settings" data-no-sfx>
  <div class="sound-toolbar">
    <div class="sound-tabs" aria-label="Типы звуков"><button class:active={tab === 'sfx'} onclick={() => changeTab('sfx')}>Эффекты ({effects.length})</button><button class:active={tab === 'voice'} onclick={() => changeTab('voice')}>Голоса ({voices.length})</button></div>
    <input type="search" aria-label="Поиск звуков" placeholder="Звук, файл или реплика…" bind:value={query}/>
    <button class="gloss small" disabled={!playing} onclick={stop}>■ Остановить</button>
  </div>
  <details class="volume-details"><summary onclick={choiceSound}>Громкость и тихий режим</summary><div class="sound-volumes">
    <NumberSetting {arrowSound} value={settings.volume} label="Громкость голоса" type="range" min={0} max={1} step={0.05} save={value => save({ volume: value })}/>
    <NumberSetting {arrowSound} value={settings.sfxVolume} label="Громкость эффектов" type="range" min={0} max={1} step={0.05} save={value => save({ sfxVolume: value })}/>
  </div>
  <label class="check-row"><input type="checkbox" checked={settings.quietMode} onchange={e => { void save({ quietMode: e.currentTarget.checked }).catch(e => error = String(e)); }}/> Тихий режим — отключить голоса и эффекты</label>
  </details>
  {#if settings.quietMode}<p class="info-note">Прослушивание отключено тихим режимом. Выключите его, чтобы проверить звук.</p>{/if}
  {#if error}<p class="sound-error" role="alert">{error}</p>{/if}
  {#if busy}<p class="muted" role="status">Сохранение звука…</p>{/if}
  {#if tab === 'sfx'}
    <p class="muted small-text">{Object.keys(settings.sfxBindings ?? {}).length ? 'Изменения сохраняются автоматически.' : 'Назначений нет — эффекты не воспроизводятся автоматически.'}</p>
    <details class="binding-details"><summary onclick={choiceSound}>Назначения эффектов ({Object.keys(settings.sfxBindings ?? {}).length})</summary>
    <div class="sound-table-wrap"><table class="sound-table" aria-label="Назначения звуковых эффектов"><thead><tr><th>Когда</th><th>Звук</th><th>Проверка</th></tr></thead><tbody>
      {#each soundEvents.filter(event => matches(event.name, settings.sfxBindings?.[event.id] ?? '')) as event (event.id)}
        <tr><td>{event.name}</td><td><select aria-label={`Звук: ${event.name}`} value={settings.sfxBindings?.[event.id] ?? ''} onchange={e => { void bind(event.id, e.currentTarget.value); }}><option value="">Без звука</option>{#each effects as effect}<option value={effect.id}>{effect.name}</option>{/each}</select></td><td><button class="gloss small" disabled={settings.quietMode || !settings.sfxBindings?.[event.id]} aria-label={`Прослушать событие: ${event.name}`} onclick={() => { void play('sfx', settings.sfxBindings[event.id]); }}>▶</button></td></tr>
      {:else}<tr><td colspan="3">Ничего не найдено.</td></tr>{/each}
    </tbody></table></div></details>
    <div class="sound-heading"><h3>Файлы эффектов</h3><label class="gloss small file-button">+ Добавить эффект<input aria-label="Добавить звуковые эффекты" type="file" multiple accept=".wav,.ogg,audio/wav,audio/ogg" disabled={busy} onchange={e => { void files(e, 'sfx'); }}/></label></div>
    <div class="sound-table-wrap"><table class="sound-table" aria-label="Файлы звуковых эффектов"><thead><tr><th>Название / файл</th><th>Длина</th><th>Используется</th><th>Управление</th></tr></thead><tbody>
      {#each effects.filter(row => matches(row.name, row.originalFile, row.id)) as row (row.id)}
        <tr><td><b>{row.name}</b><small class="file-name" title={row.path}>{row.custom ? fileName(row.path) : row.originalFile}</small><small>{row.custom ? 'Локальная копия' : 'Встроенный файл'}</small></td><td>{row.custom ? '—' : `${(row.durationMs / 1000).toLocaleString('ru-RU', { maximumFractionDigits: 3 })} с`}</td><td title={usage(row.id).join(', ')}>{usage(row.id).length ? `${usage(row.id).length} назнач.` : 'Не назначен'}</td><td><div class="sound-actions"><button class="gloss small" disabled={settings.quietMode || busy} aria-label={`Прослушать ${row.originalFile}`} onclick={() => { void play('sfx', row.id); }}>{playing === `sfx:${row.id}` ? '■' : '▶'}</button><label class="gloss small file-button">Заменить<input aria-label={`Заменить эффект ${row.originalFile}`} type="file" accept=".wav,.ogg,audio/wav,audio/ogg" disabled={busy} onchange={e => { void files(e, 'sfx', row.id); }}/></label>{#if row.custom}<button class="text-button" disabled={busy} onclick={() => { void clear('sfx', row.id); }}>{row.builtin ? 'Вернуть встроенный' : 'Убрать из списка'}</button>{/if}</div></td></tr>
      {:else}<tr><td colspan="4">Ничего не найдено.</td></tr>{/each}
    </tbody></table></div>
  {:else}
    <div class="sound-heading"><h3>Голоса и реплики</h3><label class="gloss small file-button">+ Добавить голоса<input aria-label="Добавить голоса" type="file" multiple accept=".wav,.ogg,audio/wav,audio/ogg" disabled={busy} onchange={e => { void files(e, 'voice'); }}/></label></div>
    <p class="info-note">ID связывает файл с репликой. «Заменить» назначает выбранный файл этой строке независимо от его имени. Массовый импорт использует имена файлов как ID.</p>
    <div class="sound-table-wrap"><table class="sound-table" aria-label="Голоса персонажа"><thead><tr><th>Событие / ID</th><th>Реплика</th><th>Файл</th><th>Управление</th></tr></thead><tbody>
      {#each voices.filter(row => matches(row.id, row.event, row.text, row.path ?? '')) as row (row.id)}
        <tr><td><b>{row.event}</b><small>{row.id}{row.uses > 1 ? ` · ${row.uses} репл.` : ''}</small></td><td class="voice-text">{row.text || 'Нет связи с текущим каталогом'}</td><td><span class="file-name" title={row.path ?? ''}>{row.path ? fileName(row.path) : 'Нет файла'}</span></td><td><div class="sound-actions"><button class="gloss small" disabled={settings.quietMode || !row.path || busy} aria-label={`Прослушать голос ${row.id}`} onclick={() => { void play('voice', row.id); }}>{playing === `voice:${row.id}` ? '■' : '▶'}</button><label class="gloss small file-button">{row.path ? 'Заменить' : 'Загрузить'}<input aria-label={`Загрузить голос ${row.id}`} type="file" accept=".wav,.ogg,audio/wav,audio/ogg" disabled={busy} onchange={e => { void files(e, 'voice', row.id); }}/></label>{#if row.path}<button class="text-button" disabled={busy} aria-label={`Отвязать голос ${row.id}`} onclick={() => { void clear('voice', row.id); }}>Отвязать</button>{/if}</div></td></tr>
      {:else}<tr><td colspan="4">Ничего не найдено.</td></tr>{/each}
    </tbody></table></div>
  {/if}
  <p class="muted small-text">WAV/OGG, до 20 МиБ. Убираются только связи в настройках: исходные файлы и ранее созданные копии на диске не удаляются. Прослушивание не влияет на таймер.</p>
</div>

<style>
  .sound-toolbar,.sound-heading{display:flex;align-items:center;justify-content:space-between;gap:8px;flex-wrap:wrap;margin:12px 0}
  .sound-toolbar input{flex:1;min-width:130px;font-size:11px}
  .binding-details{margin-bottom:16px}.binding-details summary{cursor:pointer;font-size:12px;color:#3b8178;padding:6px 0}.binding-details[open] summary{margin-bottom:8px}
  .volume-details{margin:8px 0 12px}.volume-details summary{cursor:pointer;font-size:11px;color:#4a8278;padding:4px 0}.volume-details[open] summary{margin-bottom:8px}
  .sound-tabs{display:flex;gap:4px}.sound-tabs button{border:1px solid #a4cbc0;border-radius:5px;padding:6px 9px;background:#f4fff7;font-size:11px}.sound-tabs button.active{background:#d7eee3;color:#226c70}
  .sound-volumes{display:grid;grid-template-columns:1fr 1fr;gap:20px;font-size:11px}.sound-volumes :global(input){width:100%}
  .sound-table-wrap{overflow-x:auto;border:1px solid #bed9cd;border-radius:6px;margin-bottom:18px;background:#fcfff9a6}
  .sound-table{width:100%;border-collapse:collapse;font-size:10px}.sound-table th,.sound-table td{padding:8px 7px;border-bottom:1px solid #d6e7dc;text-align:left;vertical-align:top}.sound-table th{background:#e5f3e9;font-weight:normal;color:#4a8278}.sound-table tr:last-child td{border-bottom:0}.sound-table b{font-weight:normal}
  .sound-table select{width:100%;font-size:10px;padding:5px;min-width:120px}.sound-table small{display:block;color:#79958b;font-size:9px;margin-top:3px}.file-name{max-width:170px;overflow-wrap:anywhere}.voice-text{min-width:130px;max-width:230px;line-height:1.5;color:#577e74}
  .sound-actions{display:flex;gap:5px;flex-wrap:wrap;align-items:center;min-width:110px}.sound-actions :global(.file-button){font-size:10px}.sound-heading h3{margin:0}.sound-error{padding:8px;border:1px solid #d0a0ad;color:#98455c;border-radius:5px;font-size:11px}
</style>
