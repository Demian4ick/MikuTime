import type { Settings } from './types.ts';
export interface AudioSource { url: string; release: () => void }
export type AudioUrl = (path: string) => string | Promise<AudioSource>;

/** One lazily loaded audio element. Never preload a voice library. */
export class VoiceManager {
  private audio: HTMLAudioElement | null = null;
  private finish: (() => void) | null = null;
  private label: string;
  private generation = 0;
  private volume = 1;
  constructor(label = 'Озвучка') { this.label = label; }
  onError: (message: string) => void = () => {};
  stop(): void { this.generation++; this.finish?.(); }
  configure(settings: Settings): void {
    this.volume = settings.volume;
    if (settings.quietMode) this.stop();
    else if (this.audio) this.audio.volume = settings.volume;
  }
  async play(key: string | undefined, settings: Settings, localUrl: AudioUrl, signal: AbortSignal): Promise<void> {
    this.stop();
    if (!key || settings.quietMode || !settings.voices[key] || signal.aborted) return Promise.resolve();
    const token = this.generation; this.volume = settings.volume;
    let source: string | AudioSource;
    try {
      const result = localUrl(settings.voices[key]);
      source = typeof result === 'string' ? result : await result;
    } catch (error) {
      if (token === this.generation && !signal.aborted) this.onError(`${this.label} ${key}: ${String(error)}`);
      return;
    }
    const release = () => { if (typeof source !== 'string') source.release(); };
    if (token !== this.generation || signal.aborted) { release(); return; }
    return new Promise(resolve => {
      const audio = new Audio(); this.audio = audio; audio.preload = 'none'; audio.volume = this.volume;
      let done = false;
      const finish = () => {
        if (done) return; done = true; clearTimeout(timeout);
        signal.removeEventListener('abort', finish);
        audio.onended = null; audio.onerror = null; audio.pause(); audio.removeAttribute('src'); audio.load();
        release();
        if (this.audio === audio) { this.audio = null; this.finish = null; }
        resolve();
      };
      this.finish = finish;
      const timeout = setTimeout(finish, 120_000);
      audio.onended = finish;
      audio.onerror = () => { this.onError(`${this.label} ${key}: ${audio.error?.message || `ошибка аудио ${audio.error?.code ?? '?'}`}`); finish(); };
      signal.addEventListener('abort', finish, { once: true });
      audio.src = typeof source === 'string' ? source : source.url;
      void audio.play().catch(error => { if (!done && !signal.aborted) this.onError(`${this.label} ${key}: ${String(error)}`); finish(); });
    });
  }
}
