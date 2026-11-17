/** Decorative character/dialogue hit areas are buttons for accessibility, not menu buttons. */
export function hoverButton(target: Element, related: Node | null): Element | null {
  const button = target.closest('button');
  if (!button || button.closest('[data-hover-exclude], [data-no-sfx]') || button.matches(':disabled') || (related && button.contains(related))) return null;
  return button;
}

export const exitFadeMs = 1500;
export const exitAudioLimitMs = 3500;

/** Save pending writes before exit; do not cut off a healthy sound when the fade ends first. */
export async function closePresentation(flush: () => Promise<void>, play: () => Promise<void>, fade: () => Promise<void>, finish: () => Promise<void>): Promise<void> {
  await flush();
  await Promise.all([play(), fade()]);
  await finish();
}
