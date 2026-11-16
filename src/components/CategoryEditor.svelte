<script lang="ts">
  import type { Category } from '../lib/types';
  let { category, save }: { category: Category; save: (category: Category) => Promise<void> } = $props();
  let name = $state(''), color = $state(''), archived = $state(false), focused = $state(false);
  let pending = $state(0);
  let error = $state('');
  let previous = '';
  $effect(() => {
    const key = JSON.stringify(category);
    if (key !== previous) {
      previous = key;
      if (!focused && pending === 0) { name = category.name; color = category.color; archived = category.archived; }
    }
  });
  function change() {
    if (!name.trim()) { error = 'Название не должно быть пустым.'; return; }
    error = '';
    pending++;
    void save({ id: category.id, name, color, archived }).catch(() => error = 'Не удалось сохранить категорию.').finally(() => pending--);
  }
</script>

<div class="category-editor">
  <input type="color" value={color} aria-label="Цвет категории" oninput={e => { color = e.currentTarget.value; change(); }}/>
  <input value={name} maxlength="60" required aria-label="Название категории" onfocus={() => focused = true} onblur={() => focused = false} oninput={e => { name = e.currentTarget.value; change(); }}/>
  <button class="text-button" type="button" onclick={() => { archived = !archived; change(); }}>{archived ? 'Вернуть' : 'В архив'}</button>
</div>
{#if error}<p class="muted" role="alert">{error}</p>{/if}
