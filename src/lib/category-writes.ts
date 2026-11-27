import { AutoSaver } from './autosave.ts';
import type { Category } from './types.ts';

/** Coalesce edits, never coalesce creations, and finish edits before deletion/exit. */
export class CategoryWrites {
  private savers = new Map<string, AutoSaver<Category>>();
  private deleting = new Set<string>();
  private mutations: Promise<void> = Promise.resolve();
  private write: (category: Category) => Promise<void>;
  private remove: (id: string) => Promise<void>;
  constructor(write: (category: Category) => Promise<void>, remove: (id: string) => Promise<void>) {
    this.write = write; this.remove = remove;
  }
  save(category: Category): Promise<void> {
    if (!category.id) return this.queue(() => this.write(category));
    if (this.deleting.has(category.id)) return Promise.reject(new Error('Категория удаляется.'));
    if (!this.savers.has(category.id)) this.savers.set(category.id, new AutoSaver(this.write));
    return this.savers.get(category.id)!.save(category);
  }
  delete(id: string): Promise<void> {
    if (this.deleting.has(id)) return Promise.reject(new Error('Категория уже удаляется.'));
    this.deleting.add(id);
    return this.queue(async () => {
      try {
        await this.savers.get(id)?.flush();
        await this.remove(id);
        this.savers.delete(id);
      } finally { this.deleting.delete(id); }
    });
  }
  async flush(): Promise<void> {
    await Promise.all([this.mutations, ...[...this.savers.values()].map(saver => saver.flush())]);
  }
  private queue(write: () => Promise<void>): Promise<void> {
    const request = this.mutations.catch(() => {}).then(write);
    this.mutations = request;
    return request;
  }
}
