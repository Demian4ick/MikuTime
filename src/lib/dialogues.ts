import type { DialogueFile } from './types.ts';
import { emotionFiles } from './emotions.ts';

export function validateDialogues(value: unknown): DialogueFile {
  const data = value as DialogueFile;
  if (!data || data.version !== 1 || !Array.isArray(data.dialogues) || data.dialogues.length > 500) throw new Error('Нужен version: 1 и массив dialogues (до 500).');
  const ids = new Set<string>();
  for (const d of data.dialogues) {
    if (!d.id || ids.has(d.id) || typeof d.event !== 'string' || !Number.isFinite(d.priority) || !Number.isFinite(d.cooldownMs)
      || d.cooldownMs < 0 || !Array.isArray(d.lines) || d.lines.length === 0 || d.lines.length > 20) throw new Error(`Неверная реплика: ${d.id}`);
    ids.add(d.id);
    for (const line of d.lines) {
      if (typeof line.en !== 'string' || typeof line.ru !== 'string' || line.en.length > 2000 || line.ru.length > 2000
        || !(line.emotion in emotionFiles) || (line.voice !== undefined && !/^[\w-]+$/.test(line.voice))
        || (line.durationMs !== undefined && (!Number.isFinite(line.durationMs) || line.durationMs < 100 || line.durationMs > 120000))) throw new Error(`Неверная строка: ${d.id}`);
    }
  }
  return data;
}
