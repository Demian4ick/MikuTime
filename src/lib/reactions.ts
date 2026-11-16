import type { Dialogue, DialogueFile, DialogueLine, Emotion, Settings } from './types.ts';
import { VoiceManager } from './voice.ts';
import { advanceCharacters } from './typing.ts';

export interface VisualState { emotion: Emotion; ru: string; en: string; speaking: boolean }
export function wait(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise(resolve => {
    if (signal.aborted) { resolve(); return; }
    const end = () => { clearTimeout(timer); signal.removeEventListener('abort', end); resolve(); };
    const timer = setTimeout(end, ms); signal.addEventListener('abort', end, { once: true });
  });
}

export class ReactionController {
  private queue: Dialogue[] = [];
  private played = new Map<string, number>();
  private previous = '';
  private current: Dialogue | null = null;
  private abort: AbortController | null = null;
  private voice = new VoiceManager();
  private disposed = false;
  private resting: boolean | undefined = undefined;
  private data: DialogueFile;
  private settings: () => Settings;
  private url: (path: string) => string;
  private visual: (value: VisualState) => void;
  constructor(data: DialogueFile, settings: () => Settings, url: (path: string) => string, visual: (value: VisualState) => void, onError: (message: string) => void = () => {}) {
    this.data = data; this.settings = settings; this.url = url; this.visual = visual;
    this.voice.onError = onError;
  }
  replace(data: DialogueFile): void { this.data = data; }
  setContext(resting: boolean): void {
    if (this.resting === resting) return;
    this.resting = resting;
    this.queue = this.queue.filter(d => this.allowed(d.event));
    if (this.current && !this.allowed(this.current.event)) this.abort?.abort();
    if (!this.current) this.visual(this.fallback());
  }
  private allowed(event: string): boolean {
    return this.resting
      ? !['start', 'pause', 'resume', 'focus_complete', 'first_extension', 'extension', 'long_work', 'touch'].includes(event)
      : !['break_start', 'break_touch'].includes(event);
  }
  private fallback(): VisualState {
    return this.resting
      ? { emotion: 'Calm', ru: 'Отдыхай, сэмпай. Торопиться некуда~ ♡', en: 'Rest easy, Senpai. There is no hurry.', speaking: false }
      : { emotion: 'Neutral', ru: 'Я рядом, сэмпай. Всё получится~ ♡', en: "I'm here with you, Senpai.", speaking: false };
  }
  event(event: string): void {
    if (this.disposed || !this.allowed(event)) return;
    const now = Date.now();
    const candidates = this.data.dialogues.filter(d => d.event === event && now - (this.played.get(d.id) ?? -Infinity) >= d.cooldownMs);
    const chosen = candidates.find(d => d.id !== this.previous) ?? candidates[0];
    if (!chosen) return;
    this.played.set(chosen.id, now); this.previous = chosen.id;
    if (this.current && chosen.priority > this.current.priority) { this.queue = this.queue.filter(d => d.priority >= chosen.priority); this.abort?.abort(); }
    this.queue.push(chosen); this.queue.sort((a, b) => b.priority - a.priority); this.queue = this.queue.slice(0, 6);
    if (!this.current) void this.drain();
  }
  skip(): void { this.abort?.abort(); }
  silence(): void { this.voice.stop(); }
  configure(): void { this.voice.configure(this.settings()); }
  private async line(line: DialogueLine, signal: AbortSignal): Promise<void> {
    this.visual({ emotion: line.emotion, ru: '', en: line.en, speaking: true });
    const voice = this.voice.play(line.voice, this.settings(), this.url, signal);
    const characters = Array.from(line.ru);
    let cursor = 0, previous = performance.now();
    // At most ~33 updates/sec while text is actively being typed, none otherwise.
    while (cursor < characters.length && !signal.aborted) {
      const now = performance.now();
      cursor = advanceCharacters(cursor, now - previous, this.settings().textSpeed ?? 60, characters.length);
      previous = now;
      this.visual({ emotion: line.emotion, ru: characters.slice(0, Math.floor(cursor)).join(''), en: line.en, speaking: true });
      if (cursor < characters.length) await wait(30, signal);
    }
    await Promise.all([voice, wait(line.durationMs ?? 3200, signal)]);
  }
  private async drain(): Promise<void> {
    while (this.queue.length && !this.disposed) {
      this.current = this.queue.shift()!; this.abort = new AbortController();
      for (const line of this.current.lines) { if (this.abort.signal.aborted) break; await this.line(line, this.abort.signal); }
      this.current = null; this.abort = null;
    }
    if (!this.disposed) this.visual(this.fallback());
  }
  dispose(): void { this.disposed = true; this.queue = []; this.abort?.abort(); this.voice.stop(); }
}
