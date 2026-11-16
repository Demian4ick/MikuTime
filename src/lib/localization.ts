import type { Emotion, Settings } from './types.ts';

export const emotionNames: Record<Emotion, string> = {
  Neutral: 'Нейтральная', Happy: 'Радостная', Calm: 'Спокойная', Pleased: 'Довольная',
  Surprised: 'Удивлённая', Relief: 'Облегчение', Worried: 'Обеспокоенная',
  Dissatisfied: 'Недовольная', Angry: 'Сердитая', Sad: 'Грустная',
};
export const talkativenessNames: Record<Settings['talkativeness'], string> = {
  Quiet: 'Немногословная', Balanced: 'Умеренная', Chatty: 'Разговорчивая', Disabled: 'Без инициативных реплик',
};
export function idleStatusText(status: string): string {
  if (status === 'available') return 'Доступно';
  if (status === 'connecting') return 'Подключение…';
  return 'Недоступно в текущем окружении';
}
