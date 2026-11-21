<script lang="ts">
  import { ArrowStepInput, type ArrowDirection } from '../lib/interaction';
  let { value, label, caption, min, max, step = 1, type = 'number', hint = '', save, arrowSound }: {
    value: number; label: string; caption?: string; min: number; max: number; step?: number;
    type?: 'number' | 'range'; hint?: string; save: (value: number) => Promise<void>;
    arrowSound: (direction: ArrowDirection) => void;
  } = $props();
  const arrowInput = new ArrowStepInput();
  let draft = $state<number | undefined>();
  let focused = $state(false);
  let error = $state('');
  let previousValue: number | undefined;
  $effect(() => {
    if (value !== previousValue) {
      previousValue = value;
      // Acknowledgements and clock snapshots must not overwrite typing in progress.
      if (!focused) draft = value;
    }
  });
  function input(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const previous = draft;
    draft = input.value === '' ? undefined : input.valueAsNumber;
    if (!input.validity.valid || draft === undefined || !Number.isFinite(draft)) {
      error = `Укажите число от ${min} до ${max}.`; return;
    }
    error = '';
    const direction = arrowInput.direction(previous, draft);
    void save(draft).then(() => { if (direction) arrowSound(direction); }).catch(() => { error = 'Не удалось сохранить. Измените значение, чтобы повторить.'; });
  }
</script>

<label class="number-setting">
  <span>{caption ?? label}</span>
  <input {type} {min} {max} {step} value={draft ?? ''} aria-label={label} title={hint || label}
    aria-invalid={error ? 'true' : undefined} oninput={input} onfocus={() => focused = true} onblur={() => { focused = false; arrowInput.reset(); }}
    onpointerdown={e => arrowInput.pointerDown(e.button, type)} onpointerup={() => arrowInput.pointerEnd()} onpointercancel={() => arrowInput.pointerEnd()} onpointerleave={() => arrowInput.pointerEnd()}
    onkeydown={e => arrowInput.keyDown(e.key, type)} onkeyup={() => arrowInput.keyUp()}/>
</label>
{#if error}<p class="input-error" role="alert">{error}</p>{/if}

<style>
  .number-setting{display:flex;flex-direction:column;gap:6px}
  .input-error{font-size:10px;color:#a25069;margin:3px 0}
</style>
