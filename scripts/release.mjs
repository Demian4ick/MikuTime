/** Release tooling: only whitelisted freshly built packages, never app-data/assets. */
import { appendFileSync, constants, copyFileSync, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { basename, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';

const stableVersion = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const exact = (pattern, value) => typeof value === 'string' && pattern.exec(value)?.[0] === value;
export function checkVersions(root, expectedTag = '') {
  const json = path => JSON.parse(readFileSync(join(root, path), 'utf8'));
  const text = path => readFileSync(join(root, path), 'utf8');
  const lock = json('package-lock.json');
  const cargo = text('src-tauri/Cargo.toml').split(/\n\[/)[0];
  const ownLock = text('src-tauri/Cargo.lock').split('[[package]]').filter(block => /^name = "miku-focus"$/m.test(block));
  if (ownLock.length !== 1) throw new Error('Cargo.lock: нужен ровно один пакет miku-focus');
  const versions = {
    'package.json': json('package.json').version,
    'package-lock.json': lock.version,
    'package-lock.json packages[""]': lock.packages?.['']?.version,
    'tauri.conf.json': json('src-tauri/tauri.conf.json').version,
    'Cargo.toml': cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1],
    'Cargo.lock': ownLock[0].match(/^version\s*=\s*"([^"]+)"/m)?.[1],
    PKGBUILD: text('packaging/PKGBUILD').match(/^pkgver=([^\r\n]+)$/m)?.[1],
  };
  const version = versions['package.json'];
  if (typeof version !== 'string' || version.length > 60 || !exact(stableVersion, version)) throw new Error('Нужна стабильная версия вида 0.1.14');
  for (const [file, value] of Object.entries(versions)) {
    if (value !== version) throw new Error(`Версии не совпадают: ${file}=${value}, ожидается ${version}`);
  }
  const tag = `v${version}`;
  if (expectedTag && expectedTag !== tag) throw new Error(`Тег ${expectedTag} не совпадает с версией ${tag}`);
  return { version, tag };
}

export function packageNames(version) {
  return { windows: `Miku-Focus-${version}-windows-x64-setup.exe`, arch: `miku-focus-${version}-1-x86_64.pkg.tar.zst` };
}
function ordinaryFile(path) {
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size === 0) throw new Error(`Нужен непустой обычный файл: ${path}`);
}
function outputDirectory(root) {
  const dir = join(root, 'release-assets');
  if (existsSync(dir)) {
    const stat = lstatSync(dir);
    if (!stat.isDirectory() || stat.isSymbolicLink()) throw new Error('release-assets не должен быть ссылкой');
  } else mkdirSync(dir);
  return dir;
}
export function collectPackage(root, platform) {
  const { version } = checkVersions(root);
  const names = packageNames(version);
  if (!Object.hasOwn(names, platform)) throw new Error('Платформа: windows или arch');
  let source;
  if (platform === 'windows') {
    const dir = join(root, 'src-tauri/target/release/bundle/nsis');
    const files = readdirSync(dir).filter(name => name.endsWith('.exe'));
    if (files.length !== 1) throw new Error('Ожидается ровно один NSIS-установщик, не обычный .exe');
    source = join(dir, files[0]);
  } else source = join(root, 'packaging', names.arch);
  ordinaryFile(source);
  const bytes = readFileSync(source);
  if (platform === 'windows' ? bytes.subarray(0, 2).toString() !== 'MZ' : !bytes.subarray(0, 4).equals(Buffer.from([0x28, 0xb5, 0x2f, 0xfd]))) {
    throw new Error('Неподходящая сигнатура установщика/пакета');
  }
  const output = join(outputDirectory(root), names[platform]);
  if (existsSync(output)) throw new Error(`Файл уже существует, сборка должна начинаться в чистом каталоге: ${output}`);
  copyFileSync(source, output, constants.COPYFILE_EXCL);
  return output;
}

export function finalizePackages(root, commit = '') {
  const { version, tag } = checkVersions(root);
  if (commit && !exact(/^[0-9a-f]{40}$/, commit)) throw new Error('Некорректный SHA коммита');
  const dir = outputDirectory(root), names = Object.values(packageNames(version)).sort();
  const generated = ['SHA256SUMS.txt', 'BUILD_INFO.json'];
  for (const file of readdirSync(dir)) {
    if (![...names, ...generated].includes(file)) throw new Error(`Посторонний файл не попадёт в релиз: ${file}`);
    ordinaryFile(join(dir, file));
  }
  const packages = names.map(name => {
    const path = join(dir, name); ordinaryFile(path);
    return { name, sha256: createHash('sha256').update(readFileSync(path)).digest('hex') };
  });
  writeFileSync(join(dir, 'SHA256SUMS.txt'), packages.map(file => `${file.sha256}  ${file.name}\n`).join(''));
  writeFileSync(join(dir, 'BUILD_INFO.json'), `${JSON.stringify({ version, tag, commit, packages }, null, 2)}\n`);
  return [...names, ...generated].map(name => join(dir, name));
}

/** Never edit a published release. Unknown API errors must not be treated as 404. */
export async function ensureDraft({ repo, tag, commit, token, body, request = fetch }) {
  if (!exact(/^[\w.-]+\/[\w.-]+$/, repo) || !exact(/^v\d+\.\d+\.\d+$/, tag) || !exact(/^[0-9a-f]{40}$/, commit)) throw new Error('Некорректные параметры релиза');
  if (!token) throw new Error('Нет GH_TOKEN');
  const api = `https://api.github.com/repos/${repo}/releases`;
  const headers = { Authorization: `Bearer ${token}`, Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28' };
  const response = await request(`${api}/tags/${encodeURIComponent(tag)}`, { headers });
  if (response.ok) {
    const release = await response.json();
    if (!release.draft || release.tag_name !== tag) throw new Error('Релиз уже опубликован или не совпадает: он не будет изменён');
    return release;
  }
  if (response.status !== 404) throw new Error(`GitHub API: ${response.status}, черновик не создан`);
  const created = await request(api, { method: 'POST', headers: { ...headers, 'Content-Type': 'application/json' }, body: JSON.stringify({ tag_name: tag, target_commitish: commit, name: `Miku Focus ${tag.slice(1)}`, body, draft: true, prerelease: false }) });
  if (!created.ok) throw new Error(`Не удалось создать черновик: ${created.status}`);
  return created.json();
}

async function main() {
  const root = process.cwd(), [command, platform] = process.argv.slice(2);
  if (command === 'check') {
    const { version, tag } = checkVersions(root, process.env.EXPECTED_TAG);
    if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `version=${version}\ntag=${tag}\n`);
    console.log(`Версии согласованы: ${tag}`);
  } else if (command === 'collect') console.log(collectPackage(root, platform));
  else if (command === 'finalize') console.log(finalizePackages(root, process.env.GITHUB_SHA).map(path => basename(path)).join('\n'));
  else if (command === 'publish') {
    if (process.env.GITHUB_EVENT_NAME !== 'push' || !process.env.GITHUB_REF?.startsWith('refs/tags/v')) throw new Error('Черновик создаётся только после push тега');
    const { tag } = checkVersions(root, process.env.EXPECTED_TAG);
    if (process.env.GITHUB_REF !== `refs/tags/${tag}`) throw new Error('Git ref не совпадает с версией');
    const assets = finalizePackages(root, process.env.GITHUB_SHA);
    const release = await ensureDraft({ repo: process.env.GITHUB_REPOSITORY, tag, commit: process.env.GITHUB_SHA, token: process.env.GH_TOKEN, body: readFileSync(join(root, '.github/RELEASE_TEMPLATE.md'), 'utf8') });
    const result = spawnSync('gh', ['release', 'upload', tag, ...assets, '--clobber', '--repo', process.env.GITHUB_REPOSITORY], { stdio: 'inherit' });
    if (result.error) throw result.error;
    if (result.status !== 0) throw new Error('Не удалось загрузить файлы в черновик');
    console.log(`Готов только черновик: ${release.html_url}`);
  } else throw new Error('Использование: node scripts/release.mjs check|collect windows|collect arch|finalize|publish');
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch(error => { console.error(error.message); process.exitCode = 1; });
}
