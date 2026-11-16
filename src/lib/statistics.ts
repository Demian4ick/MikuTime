import type { Segment, Session, Snapshot } from './types.ts';

export function dayKey(timestamp: number): string {
  const d = new Date(timestamp);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}
export function duration(segments: Segment[]): number {
  return segments.reduce((sum, s) => sum + Math.max(0, s.endMs - s.startMs), 0);
}
// Split at *local* midnight; new Date(y,m,d+1) also handles 23/25h DST days.
export function splitDays(segment: Segment): Map<string, number> {
  const days = new Map<string, number>();
  let cursor = segment.startMs;
  while (cursor < segment.endMs) {
    const d = new Date(cursor);
    const end = Math.min(segment.endMs, new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1).getTime());
    days.set(dayKey(cursor), (days.get(dayKey(cursor)) ?? 0) + end - cursor);
    cursor = end;
  }
  return days;
}
export interface DayStats { workMs: number; breakMs: number; completed: number; categories: Record<string, number>; sessions: Session[] }
function blank(): DayStats { return { workMs: 0, breakMs: 0, completed: 0, categories: {}, sessions: [] }; }

export class StatisticsService {
  days = new Map<string, DayStats>();
  totalWorkMs = 0;
  completed = 0;
  completedWorkMs = 0;
  constructor(snapshot: Pick<Snapshot, 'sessions' | 'breaks' | 'timer' | 'now'>) {
    const sessions = [...snapshot.sessions];
    const current = snapshot.timer.session;
    if (current) {
      const segments = [...current.segments];
      if (snapshot.timer.status === 'Focus' && snapshot.timer.segmentStartedAt !== null) {
        segments.push({ startMs: snapshot.timer.segmentStartedAt, endMs: Math.min(snapshot.now, snapshot.timer.deadlineMs ?? snapshot.now) });
      }
      sessions.push({ ...current, segments });
    }
    for (const session of sessions) {
      const touched = new Set<string>();
      for (const segment of session.segments) {
        for (const [day, ms] of splitDays(segment)) {
          const stats = this.get(day);
          stats.workMs += ms;
          stats.categories[session.categoryId] = (stats.categories[session.categoryId] ?? 0) + ms;
          touched.add(day);
          this.totalWorkMs += ms;
        }
      }
      for (const key of touched) this.get(key).sessions.push(session);
      if (session.completed && session.endedAt !== null) { this.get(dayKey(session.endedAt)).completed++; this.completed++; this.completedWorkMs += duration(session.segments); }
    }
    const breaks = [...snapshot.breaks];
    if (snapshot.timer.status === 'Break' && snapshot.timer.segmentStartedAt !== null) {
      breaks.push({ startMs: snapshot.timer.segmentStartedAt, endMs: Math.min(snapshot.now, snapshot.timer.deadlineMs ?? snapshot.now) });
    }
    for (const segment of breaks) for (const [key, ms] of splitDays(segment)) this.get(key).breakMs += ms;
  }
  get(key: string): DayStats { let d = this.days.get(key); if (!d) { d = blank(); this.days.set(key, d); } return d; }
  get averageMs(): number { return this.completed ? this.completedWorkMs / this.completed : 0; }
  series(period: 'days' | 'weeks' | 'months', now: number): { label: string; ms: number }[] {
    const count = period === 'days' ? 14 : period === 'weeks' ? 8 : 6;
    return Array.from({ length: count }, (_, i) => {
      const d = new Date(now); d.setHours(0, 0, 0, 0);
      if (period === 'months') { d.setDate(1); d.setMonth(d.getMonth() - count + 1 + i); }
      else if (period === 'weeks') { d.setDate(d.getDate() - ((d.getDay() + 6) % 7) - (count - 1 - i) * 7); }
      else d.setDate(d.getDate() - count + 1 + i);
      const start = dayKey(d.getTime());
      const end = new Date(d);
      if (period === 'months') end.setMonth(end.getMonth() + 1); else end.setDate(end.getDate() + (period === 'weeks' ? 7 : 1));
      const ms = [...this.days].reduce((sum, [key, s]) => key >= start && key < dayKey(end.getTime()) ? sum + s.workMs : sum, 0);
      return { label: d.toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' }), ms };
    });
  }
}
export function formatDuration(ms: number): string {
  const minutes = Math.floor(Math.max(0, ms) / 60_000);
  return minutes >= 60 ? `${Math.floor(minutes / 60)} ч ${minutes % 60} мин` : `${minutes} мин`;
}
export function formatTimer(ms: number): string {
  const seconds = Math.ceil(Math.max(0, ms) / 1000);
  return `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`;
}
// Spreadsheet formula injection protection is in addition to CSV escaping.
export function csvCell(value: string | number): string {
  let text = String(value);
  if (/^[=+\-@\t\r]/.test(text)) text = `'${text}`;
  return `"${text.replaceAll('"', '""')}"`;
}
