import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { checkVersions, collectPackage, ensureDraft, finalizePackages, packageNames } from '../scripts/release.mjs';

const version = '0.1.13', commit = 'a'.repeat(40);
function fixture() {
  const parent = resolve('.local-test'); mkdirSync(parent, { recursive: true });
  const root = mkdtempSync(join(parent, 'release-unit-'));
  const write = (path: string, data: string | Buffer) => { mkdirSync(join(root, path, '..'), { recursive: true }); writeFileSync(join(root, path), data); };
  write('package.json', JSON.stringify({ version }));
  write('package-lock.json', JSON.stringify({ version, packages: { '': { version } } }));
  write('src-tauri/tauri.conf.json', JSON.stringify({ version }));
  write('src-tauri/Cargo.toml', `[package]\nname = "miku-focus"\nversion = "${version}"\n\n[dependencies]\nx = "9"\n`);
  write('src-tauri/Cargo.lock', `version = 4\n\n[[package]]\nname = "miku-focus"\nversion = "${version}"\n`);
  write('packaging/PKGBUILD', `pkgname=miku-focus\npkgver=${version}\npkgrel=1\n`);
  return { root, write, close: () => rmSync(root, { recursive: true, force: true }) };
}
function binaries(f: ReturnType<typeof fixture>) {
  f.write('src-tauri/target/release/bundle/nsis/Miku Focus_setup.exe', Buffer.from('MZinstaller-fixture'));
  f.write(`packaging/${packageNames(version).arch}`, Buffer.from([0x28, 0xb5, 0x2f, 0xfd, 1, 2, 3]));
}

test('release versions agree across npm, both lockfiles, Tauri, Cargo and Arch', () => {
  const f = fixture();
  try {
    assert.deepEqual(checkVersions(f.root, `v${version}`), { version, tag: `v${version}` });
    assert.throws(() => checkVersions(f.root, 'v9.0.0'), /Тег/);
    f.write('src-tauri/Cargo.lock', '[[package]]\nname = "miku-focus"\nversion = "9.0.0"\n');
    assert.throws(() => checkVersions(f.root), /Cargo.lock/);
  } finally { f.close(); }
});
test('metadata rejects inconsistent package-lock and prerelease/untrusted version text', () => {
  const f = fixture();
  try {
    f.write('package-lock.json', JSON.stringify({ version, packages: { '': { version: '0.0.0' } } }));
    assert.throws(() => checkVersions(f.root), /package-lock/);
    for (const bad of ['0.1.14-beta.1', '01.1.1', '1.2.3\n', '1.2.3\nextra=value', '$(echo secret)']) {
      f.write('package.json', JSON.stringify({ version: bad })); assert.throws(() => checkVersions(f.root));
    }
  } finally { f.close(); }
});
test('package collection has an exact allowlist and never collects local user files', () => {
  const f = fixture();
  try {
    binaries(f); f.write('assets/private.wav', 'private'); f.write('state.json', 'private');
    const windows = collectPackage(f.root, 'windows'), arch = collectPackage(f.root, 'arch');
    assert.deepEqual(readdirSync(join(f.root, 'release-assets')).sort(), Object.values(packageNames(version)).sort());
    assert.ok(readFileSync(windows).equals(Buffer.from('MZinstaller-fixture')));
    assert.ok(readFileSync(arch).equals(Buffer.from([0x28, 0xb5, 0x2f, 0xfd, 1, 2, 3])));
    assert.throws(() => collectPackage(f.root, 'toString'), /Платформа/);
    assert.throws(() => collectPackage(f.root, 'windows'), /уже существует/);
  } finally { f.close(); }
});
test('ambiguous NSIS files or invalid package magic stop collection', () => {
  const f = fixture();
  try {
    binaries(f); f.write('src-tauri/target/release/bundle/nsis/second.exe', 'MZother');
    assert.throws(() => collectPackage(f.root, 'windows'), /ровно один/);
    f.write(`packaging/${packageNames(version).arch}`, 'not a package');
    assert.throws(() => collectPackage(f.root, 'arch'), /сигнатура/);
  } finally { f.close(); }
});
test('finalizing requires both builds and creates correct SHA-256 plus source commit', () => {
  const f = fixture();
  try {
    binaries(f); collectPackage(f.root, 'windows');
    assert.throws(() => finalizePackages(f.root, commit));
    collectPackage(f.root, 'arch');
    const files = finalizePackages(f.root, commit); assert.equal(files.length, 4);
    const info = JSON.parse(readFileSync(join(f.root, 'release-assets/BUILD_INFO.json'), 'utf8'));
    assert.equal(info.commit, commit); assert.equal(info.tag, `v${version}`);
    for (const file of info.packages) {
      assert.equal(file.sha256, createHash('sha256').update(readFileSync(join(f.root, 'release-assets', file.name))).digest('hex'));
    }
    assert.equal(readFileSync(join(f.root, 'release-assets/SHA256SUMS.txt'), 'utf8').trim().split('\n').length, 2);
    assert.deepEqual(finalizePackages(f.root, commit), files);
  } finally { f.close(); }
});
test('unexpected files and invalid commits prevent release finalization', () => {
  const f = fixture();
  try {
    binaries(f); collectPackage(f.root, 'windows'); collectPackage(f.root, 'arch');
    assert.throws(() => finalizePackages(f.root, 'main'), /SHA/);
    f.write('release-assets/state.json', 'private');
    assert.throws(() => finalizePackages(f.root, commit), /Посторонний/);
  } finally { f.close(); }
});
test('symlink packages are never uploaded', { skip: process.platform === 'win32' }, () => {
  const f = fixture();
  try {
    binaries(f); const packagePath = join(f.root, 'packaging', packageNames(version).arch);
    rmSync(packagePath); f.write('private.txt', 'secret'); symlinkSync(join(f.root, 'private.txt'), packagePath);
    assert.throws(() => collectPackage(f.root, 'arch'), /обычный файл/);
  } finally { f.close(); }
});
const draftParams = () => ({ repo: 'example/MikuTime', tag: `v${version}`, commit, token: 'unit-test-not-a-real-token', body: 'Описание' });
test('a missing release creates only a draft with a validated source commit', async () => {
  const calls: Array<{ url: string; options: any }> = [];
  const request = async (url: string, options: any) => {
    calls.push({ url, options });
    return calls.length === 1 ? new Response('', { status: 404 }) : Response.json({ draft: true, tag_name: `v${version}` });
  };
  const result = await ensureDraft({ ...draftParams(), request });
  assert.equal(result.draft, true); assert.equal(calls.length, 2);
  const payload = JSON.parse(calls[1].options.body);
  assert.equal(payload.draft, true); assert.equal(payload.target_commitish, commit);
  assert.equal(calls[1].options.method, 'POST');
});
test('published releases and API errors cannot be overwritten or mistaken for missing releases', async () => {
  for (const status of [200, 403, 500]) {
    let calls = 0;
    const request = async () => { calls++; return status === 200 ? Response.json({ draft: false, tag_name: `v${version}` }) : new Response('', { status }); };
    await assert.rejects(ensureDraft({ ...draftParams(), request })); assert.equal(calls, 1);
  }
});
test('an existing draft can be reused without creating another release', async () => {
  let calls = 0;
  const request = async () => { calls++; return Response.json({ draft: true, tag_name: `v${version}`, id: 42 }); };
  assert.equal((await ensureDraft({ ...draftParams(), request })).id, 42); assert.equal(calls, 1);
});
test('bad release parameters fail before any network request', async () => {
  const request = async () => assert.fail('must not contact network');
  for (const patch of [{ repo: '../bad' }, { repo: 'example/repo\n' }, { tag: 'v1.2.3\n' }, { tag: 'v1.2.3\ncommand' }, { commit: `${commit}\n` }, { commit: 'main' }, { token: '' }]) {
    await assert.rejects(ensureDraft({ ...draftParams(), ...patch, request }));
  }
});
test('manual/local publish is refused before token lookup or package collection', () => {
  const env = { ...process.env, GITHUB_EVENT_NAME: 'workflow_dispatch' };
  const result = spawnSync(process.execPath, ['scripts/release.mjs', 'publish'], { cwd: resolve('.'), env, encoding: 'utf8' });
  assert.notEqual(result.status, 0); assert.match(result.stderr, /только после push тега/);
});
test('release workflow is opt-in, bounded, read-only until draft stage and does not cache build trees', () => {
  const workflow = readFileSync(new URL('../.github/workflows/release.yml', import.meta.url), 'utf8');
  assert.ok(workflow.includes('workflow_dispatch:')); assert.ok(workflow.includes("- 'v[0-9]*'"));
  assert.ok(!workflow.includes('pull_request:') && !workflow.includes('schedule:') && !workflow.includes('branches:'));
  assert.equal((workflow.match(/contents: write/g) ?? []).length, 1);
  assert.ok(workflow.includes('retention-days: 3')); assert.ok(!workflow.includes('actions/cache'));
  assert.ok(workflow.includes("vars.ENABLE_PRIVATE_BUILDS == 'true'"));
  assert.ok(workflow.includes('inputs.allow_private_build') && workflow.includes('default: false'));
  for (const action of workflow.matchAll(/uses: ([^\s]+)/g)) assert.match(action[1], /@[0-9a-f]{40}$/);
  assert.ok(workflow.includes('makepkg --noconfirm') && !workflow.includes('--nodeps'));
});
