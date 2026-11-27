<script lang="ts">
  import { tick } from 'svelte';
  import type { Category } from '../lib/types';
  let { categories, value, select, choiceSound }: { categories: Category[]; value: string; select: (id: string) => void; choiceSound: () => void } = $props();
  let open = $state(false);
  let host: HTMLDivElement;
  let trigger: HTMLButtonElement;
  let items = $derived(categories.filter(category => !category.archived && !category.deleted));
  let selected = $derived(items.find(category => category.id === value));
  function close(restore = false) { open = false; if (restore) trigger.focus(); }
  async function show(last = false) {
    open = true;
    await tick();
    const buttons = host.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]');
    buttons[last ? buttons.length - 1 : Math.max(0, items.findIndex(category => category.id === value))]?.focus();
  }
  function choose(id: string) { choiceSound(); select(id); close(true); }
  function menuKey(event: KeyboardEvent) {
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(true); return; }
    if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault(); event.stopPropagation();
    const buttons = [...host.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')];
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : (index + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length;
    buttons[next]?.focus();
  }
</script>

<svelte:window onclick={event => { if (open && event.target instanceof Node && !host.contains(event.target)) close(); }}/>
<div class="category-picker" bind:this={host} onfocusout={event => { if (open && event.relatedTarget instanceof Node && !host.contains(event.relatedTarget)) close(); }}>
  <span>Категория</span>
  <button class="category-trigger" bind:this={trigger} aria-label="Выбрать категорию" aria-haspopup="menu" aria-expanded={open}
    onclick={() => { choiceSound(); if (open) close(); else void show(); }}
    onkeydown={event => { if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); void show(event.key === 'ArrowUp'); } }}>
    <span class="color-dot" style:background={selected?.color}></span><span>{selected?.name ?? 'Выберите категорию'}</span><span aria-hidden="true">▾</span>
  </button>
  {#if open}
    <div class="category-menu" role="menu" tabindex="-1" aria-label="Категории работы" onkeydown={menuKey}>
      {#each items as category (category.id)}
        <button role="menuitemradio" aria-checked={category.id === value} tabindex="-1" onclick={() => choose(category.id)}>
          <span class="color-dot" style:background={category.color}></span><span>{category.name}</span><span aria-hidden="true">{category.id === value ? '✓' : ''}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .category-picker{position:relative;z-index:2}
  .category-trigger{display:flex;align-items:center;gap:7px;width:100%;font-size:11px;color:#285d66;background:#f9fff8ed;border:1px solid #add0bd;border-radius:5px;padding:5px 8px;text-align:left}
  .category-trigger>span:nth-child(2),.category-menu button>span:nth-child(2){flex:1;overflow-wrap:anywhere}
  .color-dot{flex-shrink:0}
  .category-menu{position:absolute;top:100%;left:0;right:0;max-height:min(220px,40vh);overflow-y:auto;padding:4px;margin-top:3px;border:1px solid #90bdb0;border-radius:6px;background:#f7fff8;box-shadow:0 4px 12px #285d6630}
  .category-menu button{display:flex;align-items:center;gap:7px;width:100%;border:0;border-radius:3px;padding:7px 5px;background:transparent;text-align:left;font-size:11px}
  .category-menu button[aria-checked=true]{background:#dcefe4;color:#217b80}
</style>
