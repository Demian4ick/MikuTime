export type SaveStatus = 'idle' | 'saving' | 'saved' | 'error';

/** Serialize writes and merge rapid edits; there is no idle timer or background loop. */
export class AutoSaver<T extends object> {
  private pending: T | null = null;
  private waiters: { resolve: () => void; reject: (error: unknown) => void }[] = [];
  private running = false;
  private task: Promise<void> | null = null;
  private lastError: unknown = null;
  private write: (patch: T) => Promise<void>;
  private status: (value: SaveStatus) => void;
  constructor(write: (patch: T) => Promise<void>, status: (value: SaveStatus) => void = () => {}) {
    this.write = write; this.status = status;
  }
  save(patch: T): Promise<void> {
    this.pending = { ...this.pending, ...patch };
    const result = new Promise<void>((resolve, reject) => this.waiters.push({ resolve, reject }));
    if (!this.running) this.task = this.drain();
    return result;
  }
  async flush(): Promise<void> { await this.task; if (this.lastError) throw this.lastError; }
  private async drain(): Promise<void> {
    this.running = true;
    while (this.pending) {
      const patch = this.pending, waiters = this.waiters;
      this.pending = null; this.waiters = []; this.status('saving');
      try { await this.write(patch); this.lastError = null; waiters.forEach(w => w.resolve()); this.status('saved'); }
      catch (error) { this.lastError = error; waiters.forEach(w => w.reject(error)); this.status('error'); }
    }
    this.running = false;
  }
}
