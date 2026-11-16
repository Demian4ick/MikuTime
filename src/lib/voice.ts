import type { Settings } from './types.ts';

/** One lazily loaded audio element. Never preload a voice library. */
export class VoiceManager {
  private audio: HTMLAudioElement | null = null;
  private finish: (() => void) | null = null;
  onError: (message: string) => void = () => {};
  stop(): void { this.finish?.(); }
  configure(settings: Settings): void {
    if (settings.quietMode) this.stop();
    else if (this.audio) this.audio.volume = settings.volume;
  }
  play(key: string | undefined, settings: Settings, localUrl: (path: string) => string, signal: AbortSignal): Promise<void> {
    this.stop();
    if (!key || settings.quietMode || !settings.voices[key] || signal.aborted) return Promise.resolve();
    return new Promise(resolve => {
      const audio = new Audio(); this.audio = audio; audio.preload = 'none'; audio.volume = settings.volume;
      let done = false;
      const finish = () => {
        if (done) return; done = true; clearTimeout(timeout);
        signal.removeEventListener('abort', finish);
        audio.onended = null; audio.onerror = null; audio.pause(); audio.removeAttribute('src'); audio.load();
        if (this.audio === audio) { this.audio = null; this.finish = null; }
        resolve();
      };
      this.finish = finish;
      const timeout = setTimeout(finish, 120_000);
      audio.onended = finish;
      audio.onerror = () => { this.onError(`Озвучка ${key}: ${audio.error?.message || `ошибка media ${audio.error?.code ?? '?'}`}`); finish(); };
      signal.addEventListener('abort', finish, { once: true });
      audio.src = localUrl(settings.voices[key]);
      void audio.play().catch(error => { if (!signal.aborted) this.onError(`Озвучка ${key}: ${String(error)}`); finish(); });
    });
  }
}
