/** Decorative character/dialogue hit areas are buttons for accessibility, not menu buttons. */
export function hoverButton(target: Element, related: Node | null): Element | null {
  const button = target.closest('button');
  if (!button || button.closest('[data-hover-exclude], [data-no-sfx]') || button.matches(':disabled') || (related && button.contains(related))) return null;
  return button;
}

export type ArrowDirection = -1 | 1;

/** Identify native spinner/keyboard steps, without beeping for typing, paste or slider drags. */
export class ArrowStepInput {
  private pointer = false;
  private keyboard = false;
  pointerDown(button: number, type: 'number' | 'range'): void {
    this.pointer = button === 0 && type === 'number'; this.keyboard = false;
  }
  pointerEnd(): void { this.pointer = false; }
  keyDown(key: string, type: 'number' | 'range'): void {
    this.pointer = false;
    this.keyboard = ['ArrowUp', 'ArrowDown', ...(type === 'range' ? ['ArrowLeft', 'ArrowRight'] : [])].includes(key);
  }
  keyUp(): void { this.keyboard = false; }
  reset(): void { this.pointer = false; this.keyboard = false; }
  direction(previous: number | undefined, next: number | undefined): ArrowDirection | null {
    if ((!this.pointer && !this.keyboard) || previous === undefined || next === undefined || !Number.isFinite(previous) || !Number.isFinite(next) || previous === next) return null;
    return next > previous ? 1 : -1;
  }
}

export const exitFadeMs = 1500;
export const exitAudioLimitMs = 3500;

/** Save pending writes before exit; do not cut off a healthy sound when the fade ends first. */
export async function closePresentation(flush: () => Promise<void>, play: () => Promise<void>, fade: () => Promise<void>, finish: () => Promise<void>): Promise<void> {
  await flush();
  await Promise.all([play(), fade()]);
  await finish();
}
