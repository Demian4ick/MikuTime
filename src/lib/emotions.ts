import type { Emotion, Settings } from './types.ts';
export const emotionFiles: Record<Emotion, string> = {
  Neutral: 'Neutral.png', Happy: 'Happy.png', Calm: 'Calm.png', Pleased: 'pleased.png',
  Surprised: 'Surprised.png', Relief: 'relief.png', Worried: 'Worried.png',
  Dissatisfied: 'dissatisfied.png', Angry: 'Angry.png', Sad: 'Sad.png',
};
export class EmotionManager {
  static path(emotion: Emotion, settings: Settings, localUrl: (path: string) => string): string {
    return settings.sprites[emotion] ? localUrl(settings.sprites[emotion]!) : `/sprites/${emotionFiles[emotion]}`;
  }
}
