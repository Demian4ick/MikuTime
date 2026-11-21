import type { Dialogue, DialogueFile, DialogueLine, Emotion, Settings } from './types.ts';
import { VoiceManager, type AudioUrl } from './voice.ts';
import { advanceCharacters } from './typing.ts';

export interface VisualState { emotion: Emotion; ru: string; en: string; speaking: boolean; typing: boolean; awaitingInput: boolean }
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
  private finishTyping: (() => void) | null = null;
  private continueLine: (() => void) | null = null;
  private resting: boolean | undefined = undefined;
  private awaitingDecision = false;
  private data: DialogueFile;
  private settings: () => Settings;
  private url: AudioUrl;
  private visual: (value: VisualState) => void;
  private completionQuestion: boolean;
  constructor(data: DialogueFile, settings: () => Settings, url: AudioUrl, visual: (value: VisualState) => void, onError: (message: string) => void = () => {}, options: { completionQuestion?: boolean } = {}) {
    this.data = data; this.settings = settings; this.url = url; this.visual = visual;
    this.completionQuestion = options.completionQuestion !== false;
    this.voice.onError = onError;
  }
  replace(data: DialogueFile): void { this.data = data; }
  setDecision(awaiting: boolean): void {
    if (this.awaitingDecision === awaiting) return;
    this.awaitingDecision = awaiting;
    if (!this.current && !this.disposed) this.visual(this.fallback());
  }
  setContext(resting: boolean): void {
    if (this.resting === resting) return;
    this.resting = resting;
    this.queue = this.queue.filter(d => this.allowed(d.event));
    if (this.current && !this.allowed(this.current.event)) this.abort?.abort();
    if (!this.current) this.visual(this.fallback());
  }
  private allowed(event: string): boolean {
    if (event === 'focus_complete' && !this.completionQuestion) return false;
    return this.resting
      ? !['start', 'pause', 'resume', 'focus_complete', 'first_extension', 'extension', 'long_work', 'touch'].includes(event)
      : !['break_start', 'break_touch'].includes(event);
  }
  private fallback(): VisualState {
    if (this.awaitingDecision && !this.resting && this.completionQuestion) {
      const line = this.data.dialogues.find(d => d.event === 'focus_complete')?.lines[0];
      return { emotion: line?.emotion ?? 'Happy', ru: line?.ru ?? 'Ты справился, сэмпай! Ещё пять минут или заслуженный отдых? ♡', en: line?.en ?? '', speaking: false, typing: false, awaitingInput: false };
    }
    return this.resting
      ? { emotion: 'Calm', ru: 'Отдыхай. Торопиться некуда~ ♡', en: 'Rest easy. There is no hurry.', speaking: false, typing: false, awaitingInput: false }
      : { emotion: 'Neutral', ru: 'Я рядом. Всё получится~ ♡', en: "I'm here with you.", speaking: false, typing: false, awaitingInput: false };
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
  advance(): void {
    if (this.finishTyping) this.finishTyping();
    else this.continueLine?.();
  }
  silence(): void { this.voice.stop(); }
  configure(): void { this.voice.configure(this.settings()); }
  private async line(line: DialogueLine, signal: AbortSignal): Promise<void> {
    if (signal.aborted) return;
    const voiceAbort = new AbortController();
    const stopVoice = () => voiceAbort.abort();
    signal.addEventListener('abort', stopVoice, { once: true });
    void this.voice.play(line.voice, this.settings(), this.url, voiceAbort.signal);
    const characters = Array.from(line.ru);
    let cursor = 0, previous = performance.now();
    let reveal = false;
    try {
      if (characters.length) {
        this.finishTyping = () => reveal = true;
        this.visual({ emotion: line.emotion, ru: '', en: line.en, speaking: true, typing: true, awaitingInput: false });
      }
      // Updates only during typing; waiting for a click has no timeout or polling.
      while (cursor < characters.length && !signal.aborted) {
        const now = performance.now();
        cursor = reveal ? characters.length : advanceCharacters(cursor, now - previous, this.settings().textSpeed ?? 60, characters.length);
        previous = now;
        this.visual({ emotion: line.emotion, ru: characters.slice(0, Math.floor(cursor)).join(''), en: line.en, speaking: true, typing: true, awaitingInput: false });
        if (cursor < characters.length) await wait(30, signal);
      }
      this.finishTyping = null;
      if (signal.aborted) return;
      await new Promise<void>(resolve => {
        let settled = false;
        const resume = () => {
          if (settled) return;
          settled = true;
          signal.removeEventListener('abort', resume);
          if (this.continueLine === resume) this.continueLine = null;
          resolve();
        };
        this.continueLine = resume;
        signal.addEventListener('abort', resume, { once: true });
        this.visual({ emotion: line.emotion, ru: line.ru, en: line.en, speaking: true, typing: false, awaitingInput: true });
        if (signal.aborted) resume();
      });
    } finally {
      this.finishTyping = null; this.continueLine = null;
      signal.removeEventListener('abort', stopVoice);
      stopVoice();
    }
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
