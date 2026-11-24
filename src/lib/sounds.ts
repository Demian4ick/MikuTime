import catalog from './sound-catalog.json' with { type: 'json' };
import type { DialogueFile, Settings } from './types.ts';
import { builtinVoicePath, VoiceManager, loadVoiceSource, type AudioSource } from './voice.ts';
import { ReusableEffects } from './reusable-effects.ts';

export type SoundKind = 'voice' | 'sfx';
export const soundEvents = catalog.events;
export const builtinEffects = catalog.effects;
export const defaultSoundBindings = catalog.defaults;
const reactionNames: Record<string, string> = { touch: 'Обращение к Мику', break_touch: 'Общение на отдыхе', idle: 'Бездействие', return: 'Возвращение', company: 'Общение', long_work: 'Долгая работа', record: 'Личный рекорд' };
export const soundEventName = (event: string): string => soundEvents.find(e => e.id === event)?.name ?? reactionNames[event] ?? event;
export const fileName = (path: string): string => path.split(/[\\/]/).at(-1) ?? path;
export const soundUrl = (path: string, localUrl: (path: string) => string): string => path.startsWith('/sfx/') ? path : localUrl(path);
type SoundLoader = (path: string, localUrl: (path: string) => string) => string | Promise<AudioSource>;
/** WebKit cannot reliably stream media over Tauri's bundled-resource protocol. Fetch only on demand. */
export const loadSoundSource: SoundLoader = (path, localUrl) => {
  if (!builtinEffects.some(effect => effect.file === path)) return localUrl(path);
  return (async () => {
    const response = await fetch(path);
    if (!response.ok) throw new Error('Не удалось открыть встроенный звук.');
    const blob = new Blob([await response.arrayBuffer()], { type: path.endsWith('.ogg') ? 'audio/ogg' : 'audio/wav' });
    const url = URL.createObjectURL(blob);
    return { url, release: () => URL.revokeObjectURL(url) };
  })();
};

export function effectRows(settings: Settings) {
  const overrides = settings.sfx ?? {};
  return [
    ...builtinEffects.map(effect => ({ ...effect, path: overrides[effect.id] ?? effect.file, custom: !!overrides[effect.id], builtin: true })),
    ...Object.entries(overrides).filter(([id]) => !builtinEffects.some(effect => effect.id === id)).map(([id, path]) => ({ id, name: id, file: '', originalFile: fileName(path), durationMs: 0, path, custom: true, builtin: false })),
  ];
}

export function effectPath(settings: Settings, id: string): string | undefined {
  return settings.sfx?.[id] ?? builtinEffects.find(effect => effect.id === id)?.file;
}

export function voiceRows(dialogues: DialogueFile | null, settings: Settings) {
  const rows = new Map<string, { id: string; event: string; text: string; uses: number; path?: string; builtinFile?: string }>();
  for (const dialogue of dialogues?.dialogues ?? []) for (const line of dialogue.lines) {
    if (!line.voice) continue;
    const row = rows.get(line.voice);
    if (row) row.uses++;
    else rows.set(line.voice, { id: line.voice, event: soundEventName(dialogue.event), text: line.ru, uses: 1, path: settings.voices[line.voice], builtinFile: builtinVoicePath(line.voice) });
  }
  for (const [id, path] of Object.entries(settings.voices)) {
    if (!rows.has(id)) rows.set(id, { id, event: 'Не используется в диалогах', text: '', uses: 0, path, builtinFile: builtinVoicePath(id) });
  }
  return [...rows.values()];
}

/** Lazy main/hover channels: new hover feedback restarts immediately without cutting off a click or timer. */
export class SfxManager {
  private player = new VoiceManager('Эффект');
  private hoverPlayer = new VoiceManager('Наведение');
  private builtinPlayer: ReusableEffects;
  private last = new Map<string, number>();
  private currentEvent = '';
  private generation = 0;
  private settings: () => Settings | null;
  private localUrl: (path: string) => string;
  private resting = false;
  private source: SoundLoader;
  constructor(settings: () => Settings | null, localUrl: (path: string) => string, onError: (message: string) => void, source: SoundLoader = loadSoundSource) {
    this.settings = settings; this.localUrl = localUrl; this.player.onError = onError; this.hoverPlayer.onError = onError; this.source = source;
    this.builtinPlayer = new ReusableEffects(path => this.source(path, this.localUrl), onError);
  }
  private allowed(event: string): boolean {
    const context = soundEvents.find(e => e.id === event)?.context;
    return context === 'any' || context === (this.resting ? 'rest' : 'work');
  }
  configure(resting: boolean): void {
    this.resting = resting;
    const settings = this.settings();
    if (!settings) return;
    if (this.currentEvent && !this.allowed(this.currentEvent)) this.stop();
    this.player.configure({ ...settings, volume: settings.sfxVolume ?? 0.5 });
    this.hoverPlayer.configure({ ...settings, volume: settings.sfxVolume ?? 0.5 });
    if (settings.quietMode) this.builtinPlayer.stop();
    else this.builtinPlayer.configure(settings.sfxVolume ?? 0.5);
  }
  async event(event: string): Promise<void> {
    const settings = this.settings(), metadata = soundEvents.find(e => e.id === event);
    if (!settings || !metadata || settings.quietMode || !this.allowed(event)) return;
    const id = settings.sfxBindings?.[event], path = id ? effectPath(settings, id) : undefined;
    if (!id || !path || (metadata.intervalMs > 0 && Date.now() - (this.last.get(event) ?? -Infinity) < metadata.intervalMs)) return;
    const hover = event === 'choice_hover';
    this.last.set(event, Date.now());
    const token = hover ? this.generation : ++this.generation;
    if (!hover) this.currentEvent = event;
    try {
      const player = hover ? this.hoverPlayer : this.player, channel = hover ? 'hover' : 'main';
      if (this.source === loadSoundSource && builtinEffects.some(effect => effect.file === path)) {
        player.stop();
        await this.builtinPlayer.play(path, settings.sfxVolume ?? 0.5, channel);
      } else {
        this.builtinPlayer.stopChannel(channel);
        await player.play(id, { ...settings, volume: settings.sfxVolume ?? 0.5, voices: { [id]: path } }, path => this.source(path, this.localUrl), new AbortController().signal);
      }
    } finally { if (!hover && token === this.generation) this.currentEvent = ''; }
  }
  stop(): void { this.generation++; this.currentEvent = ''; this.player.stop(); this.hoverPlayer.stop(); this.builtinPlayer.stop(); }
}

/** Independent preview channel; the settings table awaits completion or cancellation. */
export class SoundPreview {
  private player = new VoiceManager('Прослушивание');
  private kind: SoundKind = 'voice';
  private source: SoundLoader;
  constructor(onError: (message: string) => void, source: SoundLoader = loadSoundSource) { this.player.onError = onError; this.source = source; }
  play(kind: SoundKind, id: string, settings: Settings, localUrl: (path: string) => string): Promise<void> {
    this.kind = kind;
    const path = kind === 'voice' ? settings.voices[id] : effectPath(settings, id);
    return this.player.play(id, { ...settings, volume: kind === 'voice' ? settings.volume : settings.sfxVolume, voices: path ? { [id]: path } : {} }, (path, signal) => kind === 'voice' && this.source === loadSoundSource ? loadVoiceSource(path, localUrl, signal) : this.source(path, localUrl), new AbortController().signal);
  }
  configure(settings: Settings): void { this.player.configure({ ...settings, volume: this.kind === 'voice' ? settings.volume : settings.sfxVolume }); }
  stop(): void { this.player.stop(); }
}
