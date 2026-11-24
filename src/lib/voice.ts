import type { Settings } from './types.ts';
import voiceCatalog from './voice-catalog.json' with { type: 'json' };
export const builtinVoices = voiceCatalog.voices;
export const builtinVoicePath = (id: string): string | undefined => builtinVoices.find(voice => voice.id === id)?.file;
export interface AudioSource { url: string; release: () => void }
export type AudioUrl = (path: string, signal?: AbortSignal) => string | Promise<AudioSource>;

/** Both bundled and imported WAV use a lazy typed Blob; fetch only the active line. */
export async function loadVoiceSource(path: string, localUrl: (path: string) => string, signal?: AbortSignal): Promise<AudioSource> {
  const response = await fetch(builtinVoices.some(voice => voice.file === path) ? path : localUrl(path), { signal });
  if (!response.ok) throw new Error('Не удалось открыть локальную озвучку.');
  const limit = 20 * 1024 * 1024;
  if (Number(response.headers.get('content-length')) > limit) throw new Error('Файл озвучки больше 20 МиБ.');
  const bytes = await response.arrayBuffer();
  if (bytes.byteLength > limit) throw new Error('Файл озвучки больше 20 МиБ.');
  const blob = new Blob([bytes], { type: path.toLowerCase().endsWith('.ogg') ? 'audio/ogg' : 'audio/wav' });
  const url = URL.createObjectURL(blob);
  return { url, release: () => URL.revokeObjectURL(url) };
}

/** One lazily loaded audio element. Never preload a voice library. */
export class VoiceManager {
  private audio: HTMLAudioElement | null = null;
  private finish: (() => void) | null = null;
  private label: string;
  private generation = 0;
  private volume = 1;
  private loading: AbortController | null = null;
  constructor(label = 'Озвучка') { this.label = label; }
  onError: (message: string) => void = () => {};
  stop(): void { this.generation++; this.loading?.abort(); this.loading = null; this.finish?.(); }
  configure(settings: Settings): void {
    this.volume = settings.volume;
    if (settings.quietMode) this.stop();
    else if (this.audio) this.audio.volume = settings.volume;
  }
  async play(key: string | undefined, settings: Settings, localUrl: AudioUrl, signal: AbortSignal): Promise<void> {
    this.stop();
    if (!key || settings.quietMode || !settings.voices[key] || signal.aborted) return Promise.resolve();
    const token = this.generation; this.volume = settings.volume;
    const loading = new AbortController(); this.loading = loading;
    const abortLoading = () => loading.abort();
    signal.addEventListener('abort', abortLoading, { once: true });
    let source: string | AudioSource;
    try {
      const result = localUrl(settings.voices[key], loading.signal);
      source = typeof result === 'string' ? result : await result;
    } catch (error) {
      if (token === this.generation && !signal.aborted) this.onError(`${this.label} ${key}: ${String(error)}`);
      return;
    } finally {
      signal.removeEventListener('abort', abortLoading);
      if (this.loading === loading) this.loading = null;
    }
    const release = () => { if (typeof source !== 'string') source.release(); };
    if (token !== this.generation || signal.aborted || loading.signal.aborted) { release(); return; }
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
