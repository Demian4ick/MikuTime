<script lang="ts">
  import type { VisualState } from '../lib/reactions';
  let { visual, advance, skip }: { visual: VisualState; advance: () => void; skip: () => void } = $props();
</script>

<section class="dialogue-panel" data-hover-exclude aria-label="Русские субтитры">
  <div class="nameplate">Хацунэ Мику <span>Твой помощник</span><i>♡</i></div>
  <p class="subtitle" id="dialogue-text" aria-live="off">{visual.ru}{#if visual.typing}<span class="type-cursor">▏</span>{/if}</p>
  {#if visual.awaitingInput}<span class="continue-note">Нажмите, чтобы продолжить ▸</span>{/if}
  <span class="dialogue-corner">✧</span>
  <button class="dialogue-advance" type="button" aria-label="Продолжить диалог" aria-describedby="dialogue-text" disabled={!visual.speaking} onclick={advance}
    title={visual.typing ? 'Нажмите, чтобы показать реплику целиком' : 'Нажмите, чтобы продолжить диалог'}>
  </button>
  <button class="skip-line" title="Пропустить реплику" onclick={skip}>Пропустить »</button>
</section>

<style>
  .dialogue-advance{position:absolute;inset:0;display:block;width:100%;height:100%;border:0;border-radius:inherit;padding:0;background:transparent;z-index:1}
  .dialogue-advance:disabled{opacity:1;cursor:default}
  .skip-line{z-index:2}
  .continue-note{position:absolute;left:25px;bottom:14px;font-size:9px;color:#60988d}
</style>
