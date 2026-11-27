import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = new URL('../', import.meta.url);
const documents = ['README.md', 'PROGRESS.md', 'PERFORMANCE.md', '.github/BETA_NOTES.md', '.github/RELEASE_TEMPLATE.md'];
function collect(directory: string) {
  for (const entry of readdirSync(new URL(directory, root), { withFileTypes: true })) {
    const path = `${directory}${entry.name}`;
    if (entry.isDirectory()) collect(`${path}/`);
    else if (entry.name.endsWith('.md')) documents.push(path);
  }
}
collect('docs/'); collect('integrations/obsidian/');

test('public documentation contains instructions rather than conversation history or private machine paths', () => {
  for (const path of documents) {
    const content = readFileSync(new URL(path, root), 'utf8');
    assert.doesNotMatch(content, /по\s+(?:новому\s+|прямому\s+|отдельному\s+|явному\s+|последнему\s+)*(?:запросу|уточнению)\s+(?:пользователя|владельца)/i, path);
    assert.doesNotMatch(content, /согласованн|ноутбук[ае]\s+автора|агентом|\/home\/demian4ick|\/mnt\/obsidian\/Vault|\.local-test\/[^\s`]+-\d{12,}/i, path);
  }
});

test('local Markdown links resolve after reorganizing the instructions', () => {
  for (const path of documents) {
    const document = new URL(path, root);
    const content = readFileSync(document, 'utf8');
    for (const [, target] of content.matchAll(/\]\(([^\s)]+)\)/g)) {
      if (/^(?:https?:|#|mailto:)/.test(target)) continue;
      const url = new URL(target.split('#')[0], document);
      assert.ok(existsSync(fileURLToPath(url)), `${path}: ${target}`);
    }
  }
});
