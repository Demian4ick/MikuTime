export type Status = 'Idle' | 'Focus' | 'Paused' | 'Break' | 'AwaitingDecision' | 'Completed';
export type Phase = 'Focus' | 'ShortBreak' | 'LongBreak';
export type Emotion = 'Neutral' | 'Happy' | 'Calm' | 'Pleased' | 'Surprised' | 'Relief' | 'Worried' | 'Dissatisfied' | 'Angry' | 'Sad';
export interface Segment { startMs: number; endMs: number }
export interface Session { id: string; categoryId: string; startedAt: number; endedAt: number | null; segments: Segment[]; extensions: number; completed: boolean }
export interface Category { id: string; name: string; color: string; archived: boolean; deleted?: boolean }
export interface Settings {
  focusMinutes: number; shortBreakMinutes: number; longBreakMinutes: number; sessionsBeforeLong: number;
  autoStart: boolean; volume: number; textSpeed: number; quietMode: boolean; talkativeness: 'Quiet' | 'Balanced' | 'Chatty' | 'Disabled';
  dailyGoalMinutes: number; alwaysOnTop: boolean; companionPosition: [number, number] | null; companionSize: [number, number] | null;
  backgroundMode: 'Companion' | 'Panel';
  background: string | null; sprites: Partial<Record<Emotion, string>>; voices: Record<string, string>;
  sfx: Record<string, string>; sfxBindings: Record<string, string>; sfxVolume: number;
}
export type SettingsPatch = Partial<Pick<Settings, 'focusMinutes' | 'shortBreakMinutes' | 'longBreakMinutes' | 'sessionsBeforeLong' | 'autoStart' | 'volume' | 'sfxVolume' | 'textSpeed' | 'quietMode' | 'talkativeness' | 'dailyGoalMinutes' | 'alwaysOnTop' | 'backgroundMode'>>;
export interface Timer {
  status: Status; phase: Phase; remainingMs: number; deadlineMs: number | null;
  segmentStartedAt: number | null; session: Session | null; cycle: number;
}
export interface ReactionEvent { id: number; event: string }
export interface ObsidianExportStatus {
  folder: string | null; file: string | null; sourceOs: string; sourceId: string;
  exporting: boolean; lastExportedAt: number | null; error: string | null;
}
export interface Snapshot {
  now: number; remainingMs: number; timer: Timer; settings: Settings; categories: Category[];
  selectedCategory: string; sessions: Session[]; breaks: Segment[]; activeWindow: string;
  reactions: ReactionEvent[]; recovered: boolean; idleAdapter: string; systemIdle: boolean;
  platform: string; storagePath: string; error: string | null;
  rendererCompatibility: boolean;
  quitting: boolean;
  obsidian: ObsidianExportStatus;
  panel: { error: string | null; waybarConfig: string };
}
export interface DialogueLine { en: string; ru: string; emotion: Emotion; voice?: string; durationMs?: number }
export interface Dialogue { id: string; event: string; priority: number; cooldownMs: number; lines: DialogueLine[] }
export interface DialogueFile { version: number; dialogues: Dialogue[] }
