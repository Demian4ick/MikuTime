<script lang="ts">
  import { tick } from 'svelte';
  import type { Category } from '../lib/types';
  let { category, save, remove, lastActive }: {
    category: Category; save: (category: Category) => Promise<void>; remove: (id: string) => Promise<void>; lastActive: boolean;
  } = $props();
  let name = $state(''), color = $state(''), archived = $state(false), focused = $state(false);
  let pending = $state(0);
  let error = $state('');
  let confirming = $state(false), deleting = $state(false);
  let confirmButton = $state<HTMLButtonElement>(), deleteButton = $state<HTMLButtonElement>();
  let previous = '';
  $effect(() => {
    const key = JSON.stringify(category);
    if (key !== previous) {
      previous = key;
      if (!focused && pending === 0) { name = category.name; color = category.color; archived = category.archived; }
    }
  });
  function change() {
    if (deleting || confirming) return;
    if (!name.trim()) { error = 'Название не должно быть пустым.'; return; }
    error = '';
    pending++;
    void save({ id: category.id, name, color, archived }).catch(e => { error = String(e); archived = category.archived; }).finally(() => pending--);
  }
  async function confirm() { confirming = true; error = ''; await tick(); confirmButton?.focus(); }
  function cancel() { if (!deleting) { confirming = false; deleteButton?.focus(); } }
  async function deleteCategory() {
    if (deleting) return;
    deleting = true; error = '';
    try { await remove(category.id); }
    catch (e) { error = String(e); deleting = false; }
  }
</script>

<svelte:window onkeydown={e => { if (confirming && e.key === 'Escape') { e.preventDefault(); cancel(); } }}/>
<div class="category-editor">
  <input type="color" value={color} disabled={confirming} aria-label={`Цвет категории ${category.name}`} oninput={e => { color = e.currentTarget.value; change(); }}/>
  <input value={name} maxlength="60" required disabled={confirming} aria-label={`Название категории ${category.name}`} onfocus={() => focused = true} onblur={() => focused = false} oninput={e => { name = e.currentTarget.value; change(); }}/>
  <button class="text-button" type="button" disabled={confirming || lastActive} title={lastActive ? 'Сначала создайте ещё одну активную категорию' : ''} aria-label={`${archived ? 'Вернуть из архива' : 'В архив'}: ${category.name}`} onclick={() => { archived = !archived; change(); }}>{archived ? 'Вернуть' : 'В архив'}</button>
  <button class="text-button" type="button" bind:this={deleteButton} disabled={confirming || lastActive} title={lastActive ? 'Сначала создайте ещё одну активную категорию' : ''} aria-label={`Удалить категорию ${category.name}`} onclick={() => { void confirm(); }}>Удалить</button>
</div>
{#if confirming}
  <div class="category-delete-confirm" role="group" aria-label={`Удаление категории ${category.name}`}>
    <p>Удалить «{category.name}»? Прежние занятия останутся в дневнике с этим названием и цветом.</p>
    <div class="inline-actions"><button class="gloss small" bind:this={confirmButton} disabled={deleting || lastActive} onclick={() => { void deleteCategory(); }}>{deleting ? 'Удаление…' : 'Да, удалить'}</button><button class="text-button" disabled={deleting} onclick={cancel}>Отмена</button></div>
  </div>
{/if}
{#if error}<p class="muted" role="alert">{error}</p>{/if}

<style>
  .category-delete-confirm { margin: 0 0 12px; padding: 10px 14px; border: 1px solid #bedacb; border-radius: 7px; background: #fffefa; }
  .category-delete-confirm p { margin: 0 0 8px; font-size: 12px; }
</style>
