import type { AudioSource } from './voice.ts';

type Channel = 'main' | 'hover';
type Source = string | AudioSource;
type Entry = { source?: Source; pending: Promise<Source> };
type Slot = { audio: HTMLAudioElement | null; path: string; finish?: () => void; generation: number };

/** Two reusable media elements, not a new player/pipeline per hover; only bundled files are cached. */
export class ReusableEffects {
  private slots: Record<Channel, Slot> = { main: { audio: null, path: '', generation: 0 }, hover: { audio: null, path: '', generation: 0 } };
  private cache = new Map<string, Entry>();
  private generation = 0;
  private volume = 0.5;
  private load: (path: string) => Source | Promise<Source>;
  private onError: (message: string) => void;
  constructor(load: (path: string) => Source | Promise<Source>, onError: (message: string) => void) { this.load = load; this.onError = onError; }
  private source(path: string): Promise<Source> {
    const cached = this.cache.get(path);
    if (cached) return cached.pending;
    const generation = this.generation;
    const entry: Entry = { pending: Promise.resolve().then(() => this.load(path)).then(source => {
      if (generation !== this.generation) { if (typeof source !== 'string') source.release(); }
      else entry.source = source;
      return source;
    }).catch(error => { if (this.cache.get(path) === entry) this.cache.delete(path); throw error; }) };
    this.cache.set(path, entry);
    return entry.pending;
  }
  configure(volume: number): void { this.volume = volume; for (const slot of Object.values(this.slots)) if (slot.audio) slot.audio.volume = volume; }
  stopChannel(channel: Channel): void {
    const slot = this.slots[channel]; slot.generation++; slot.finish?.(); slot.audio?.pause();
  }
  stop(): void {
    for (const channel of ['main', 'hover'] as const) {
      this.stopChannel(channel);
      const slot = this.slots[channel];
      if (slot.audio) { slot.audio.removeAttribute('src'); slot.audio.load(); }
      slot.audio = null; slot.path = '';
    }
    this.generation++;
    for (const entry of this.cache.values()) if (entry.source && typeof entry.source !== 'string') entry.source.release();
    this.cache.clear();
  }
  play(path: string, volume: number, channel: Channel): Promise<void> {
    this.volume = volume;
    this.stopChannel(channel);
    const slot = this.slots[channel], generation = slot.generation;
    return new Promise(resolve => {
      let done = false, timeout: ReturnType<typeof setTimeout> | undefined;
      const finish = () => {
        if (done) return;
        done = true; clearTimeout(timeout);
        if (slot.audio) { slot.audio.onended = null; slot.audio.onerror = null; }
        slot.finish = undefined; resolve();
      };
      slot.finish = finish;
      void this.source(path).then(source => {
        if (done || slot.generation !== generation) return;
        const audio = slot.audio ??= new Audio(); audio.preload = 'none'; audio.volume = this.volume;
        const url = typeof source === 'string' ? source : source.url;
        if (slot.path !== path) { audio.src = url; slot.path = path; }
        else { try { audio.currentTime = 0; } catch { audio.src = url; } }
        // Leave the ended clip loaded: pause/load/unload here would flush short queued PCM.
        audio.onended = () => { if (audio.ended) finish(); };
        audio.onerror = () => { this.onError(`Эффект: ${audio.error?.message || `ошибка аудио ${audio.error?.code ?? '?'}`}`); slot.path = ''; audio.pause(); finish(); };
        timeout = setTimeout(() => { audio.pause(); finish(); }, 120_000);
        void audio.play().catch(error => { if (!done) { this.onError(`Эффект: ${String(error)}`); finish(); } });
      }).catch(error => { if (!done) { this.onError(`Эффект: ${String(error)}`); finish(); } });
    });
  }
}
