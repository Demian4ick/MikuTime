export type Status = 'Idle' | 'Focus' | 'Paused' | 'Break' | 'AwaitingDecision' | 'Completed';
export type Phase = 'Focus' | 'ShortBreak' | 'LongBreak';
export type Emotion = 'Neutral' | 'Happy' | 'Calm' | 'Pleased' | 'Surprised' | 'Relief' | 'Worried' | 'Dissatisfied' | 'Angry' | 'Sad';
export interface Segment { startMs: number; endMs: number }
export interface Session { id: string; categoryId: string; startedAt: number; endedAt: number | null; segments: Segment[]; extensions: number; completed: boolean }
export interface Category { id: string; name: string; color: string; archived: boolean }
export interface Settings {
  focusMinutes: number; shortBreakMinutes: number; longBreakMinutes: number; sessionsBeforeLong: number;
  autoStart: boolean; volume: number; textSpeed: number; quietMode: boolean; talkativeness: 'Quiet' | 'Balanced' | 'Chatty' | 'Disabled';
  dailyGoalMinutes: number; alwaysOnTop: boolean; companionPosition: [number, number] | null;
  background: string | null; sprites: Partial<Record<Emotion, string>>; voices: Record<string, string>;
}
export type SettingsPatch = Partial<Pick<Settings, 'focusMinutes' | 'shortBreakMinutes' | 'longBreakMinutes' | 'sessionsBeforeLong' | 'autoStart' | 'volume' | 'textSpeed' | 'quietMode' | 'talkativeness' | 'dailyGoalMinutes' | 'alwaysOnTop'>>;
export interface Timer {
  status: Status; phase: Phase; remainingMs: number; deadlineMs: number | null;
  segmentStartedAt: number | null; session: Session | null; cycle: number;
}
export interface ReactionEvent { id: number; event: string }
export interface Snapshot {
  now: number; remainingMs: number; timer: Timer; settings: Settings; categories: Category[];
  selectedCategory: string; sessions: Session[]; breaks: Segment[]; activeWindow: string;
  reactions: ReactionEvent[]; recovered: boolean; idleAdapter: string; systemIdle: boolean;
  platform: string; storagePath: string; error: string | null;
  rendererCompatibility: boolean;
}
export interface DialogueLine { en: string; ru: string; emotion: Emotion; voice?: string; durationMs?: number }
export interface Dialogue { id: string; event: string; priority: number; cooldownMs: number; lines: DialogueLine[] }
export interface DialogueFile { version: number; dialogues: Dialogue[] }
