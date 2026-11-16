<script lang="ts">
  let { value, locked, adjust }: { value: number; locked: boolean; adjust: (delta: -1 | 1) => Promise<void> } = $props();
  let hint = $derived(locked ? 'Стрелки доступны до запуска работы. Следующую длительность можно изменить в настройках.' : 'Изменить длительность работы на одну минуту');
</script>

<div class="duration-arrows" role="group" aria-label="Изменить длительность работы">
  <button type="button" aria-label="Увеличить время работы на минуту" title={hint} disabled={locked || value >= 480} onclick={() => { void adjust(1).catch(() => {}); }}>
    <svg viewBox="0 0 16 12" aria-hidden="true"><path d="m4 8 4-4 4 4"/></svg>
  </button>
  <button type="button" aria-label="Уменьшить время работы на минуту" title={hint} disabled={locked || value <= 1} onclick={() => { void adjust(-1).catch(() => {}); }}>
    <svg viewBox="0 0 16 12" aria-hidden="true"><path d="m4 4 4 4 4-4"/></svg>
  </button>
</div>

<style>
  .duration-arrows{display:grid;gap:2px;flex-shrink:0}
  button{display:grid;place-items:center;width:24px;height:24px;padding:0;border:0;border-radius:4px;background:transparent;color:#438e87}
  button:hover:not(:disabled){background:#def5ea}
  button:active:not(:disabled){background:#cceade}
  button:disabled{opacity:.3}
  svg{width:15px;height:11px;fill:none;stroke:currentColor;stroke-width:1.7;stroke-linecap:round;stroke-linejoin:round}
</style>
