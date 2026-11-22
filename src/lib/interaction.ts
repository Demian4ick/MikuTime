import type { DialogueFile, DialogueLine } from './types.ts';

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
export const exitFarewellMs = 1500;
export const exitVoiceLimitMs = 2200;
export const exitFarewell = 'Я буду ждать нашей следующей встречи ♡';

export function farewellLine(data: DialogueFile | null): DialogueLine {
  return data?.dialogues.find(dialogue => dialogue.event === 'app_quit')?.lines[0]
    ?? { en: "I'll be waiting for you~", ru: exitFarewell, emotion: 'Calm', voice: 'goodbye' };
}

/** Read the line for at least 1.5s; wait for its voice, bounded even if a fetch stalls. */
export async function farewellPresentation(play: () => Promise<void>, stop: () => void, minimumMs = exitFarewellMs, voiceLimitMs = exitVoiceLimitMs): Promise<void> {
  let minimum: ReturnType<typeof setTimeout> | undefined, limit: ReturnType<typeof setTimeout> | undefined;
  try {
    const reading = new Promise<void>(resolve => minimum = setTimeout(resolve, minimumMs));
    const boundedVoice = new Promise<void>(resolve => limit = setTimeout(() => { stop(); resolve(); }, voiceLimitMs));
    await Promise.all([reading, Promise.race([play(), boundedVoice])]);
  } finally {
    clearTimeout(minimum); clearTimeout(limit); stop();
  }
}

/** Save first, then run farewell, close sound and black fade together; finish only after all three. */
export async function closePresentation(flush: () => Promise<void>, play: () => Promise<void>, fade: () => Promise<void>, finish: () => Promise<void>, farewell?: () => Promise<void>): Promise<void> {
  await flush();
  await Promise.all([farewell?.(), play(), fade()]);
  await finish();
}
