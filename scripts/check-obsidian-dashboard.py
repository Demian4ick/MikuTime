"""Private Chromium renderer check; no plugins, installs, external requests or vault writes."""
import argparse
import html
import json
import os
from pathlib import Path
import re
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument('--export', required=True, type=Path)
parser.add_argument('--width', type=int, default=1440)
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
base = root / '.local-test' / f'obsidian-dashboard-{time.time_ns()}'
base.mkdir(mode=0o700)
code = (root / 'integrations/obsidian/Панель/view.js').read_text()
css = (root / 'integrations/obsidian/Панель/view.css').read_text()
files = {'Дневник/Статистика/FocusMikuArch/Miku Focus/Данные/linux.json': args.export.read_text()}
payload = json.dumps(files, ensure_ascii=False).replace('<', '\\u003c')
page = '''<!doctype html><html lang="ru"><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'">
<style>:root{--background-primary:#181b22;--background-secondary:#20252e;--text-normal:#e3e8ee;--text-muted:#939fac;--font-interface:system-ui}body{background:var(--background-primary);margin:35px auto;padding:0 25px;max-width:1200px;color:var(--text-normal);font-family:system-ui}button{background:var(--background-secondary);color:var(--text-normal);font:inherit}h1{font-size:24px}button:hover{filter:brightness(1.2)}</style>
<style>''' + css + '''</style><h1>Miku Focus · общая статистика</h1><main id="view"></main><pre id="checks"></pre>
<script type="module">
window.addEventListener('error', event => document.body.dataset.error = event.message);
window.addEventListener('unhandledrejection', event => document.body.dataset.error = String(event.reason));
const files = ''' + payload + ''';
const listeners = new Map(), cleanup = [];
const dv = {container:document.getElementById('view'), io:{load:async path => files[path]},
  app:{vault:{on:(name, callback) => {const event={name,callback}; listeners.set(event,event); return event;},offref:event => listeners.delete(event)}},
  component:{registerEvent:event => cleanup.push(() => listeners.delete(event)), register:fn => cleanup.push(fn)}};
const input = undefined;
''' + code + '''
const checks = [];
function check(name, condition) { if (!condition) throw new Error(name); checks.push(name); }
async function settle() { for (let i=0; i<12; i++) await Promise.resolve(); }
check('dashboard renders without JS errors', document.querySelectorAll('.miku-metric').length === 4);
check('the layout fits the viewport', document.documentElement.scrollWidth <= window.innerWidth);
const sourceSnapshot = JSON.parse(files[Object.keys(files)[0]]);
check('all-time work exactly matches the real Rust snapshot', mikuModel([sourceSnapshot], 'all').workMs === sourceSnapshot.totals.workMs);
check('only real Arch data is shown; Windows is explicitly missing', document.querySelector('.miku-source').textContent.includes('Arch Linux') && document.querySelectorAll('.miku-source')[1].textContent.includes('Нет выгрузки'));
check('heatmap has 84 accessible days', document.querySelectorAll('.miku-day[aria-label]').length === 84);
check('period buttons work', (()=>{document.querySelectorAll('.miku-periods button')[0].click();return document.querySelector('.miku-periods .is-active').textContent==='7 дней';})());
check('there are no external resources', performance.getEntriesByType('resource').length === 0);
check('four vault listeners are registered', listeners.size === 4);
await mikuDashboard(dv, input);
check('Dataview rerender replaces subscriptions and its DOM', listeners.size === 4 && document.querySelectorAll('.miku-stats').length === 1 && cleanup.length === 1);
document.querySelectorAll('.miku-periods button')[0].click();
const winPath = 'Дневник/Статистика/FocusMikuWin/Miku Focus/Данные/windows.json';
const winFixture = JSON.parse(files[Object.keys(files)[0]]);
winFixture.source = {id:'private-windows-renderer-fixture',os:'windows'};
files[winPath] = JSON.stringify(winFixture);
for (const {name,callback} of listeners.values()) if(name==='create') callback({path:winPath});
await settle();
check('a Windows file appearing is picked up automatically', !document.querySelectorAll('.miku-source')[1].textContent.includes('Нет выгрузки') && [...document.querySelectorAll('.miku-bar-windows')].some(element=>parseFloat(element.style.height)>0));
delete files[winPath];
for (const {name,callback} of listeners.values()) if(name==='delete') callback({path:winPath});
await settle();
check('removing Windows leaves Arch visible and marks Windows missing', document.querySelectorAll('.miku-source')[1].textContent.includes('Нет выгрузки') && !document.querySelector('.miku-source').textContent.includes('Нет выгрузки'));
await mikuDashboard(dv, {os:'windows'});
check('a missing Windows-only source shows dashes, not fake zero results', [...document.querySelectorAll('.miku-metric strong')].every(element=>element.textContent==='—'));
await mikuDashboard(dv, input);
document.querySelectorAll('.miku-periods button')[0].click();
// Re-render via a real file event: neither a network request nor a polling timer.
const path = Object.keys(files)[0];
for (const {name,callback} of listeners.values()) if(name==='modify') callback({path});
await settle();
check('file changes preserve selected period', document.querySelector('.miku-periods .is-active').textContent==='7 дней');
document.querySelectorAll('.miku-periods button')[1].click();
// Text and color inputs never become executable HTML/CSS.
const original = files[path], hostile = JSON.parse(original);
for (const category of hostile.categories) {
  category.name = '<img src=x onerror=alert(1)>';
  category.color = 'url(https://invalid.example/test)';
}
files[path] = JSON.stringify(hostile);
document.querySelector('.miku-refresh').click();
await settle();
check('category text cannot inject HTML', document.querySelectorAll('img').length === 0);
files[path] = '{broken'; document.querySelector('.miku-refresh').click();
await settle();
check('corrupt source is visible without crashing', document.querySelector('.miku-stats').textContent.includes('Не удалось прочитать выгрузку'));
files[path] = original; document.querySelector('.miku-refresh').click();
await settle();
const originalLoad = dv.io.load, lateReads = [];
dv.io.load = path => new Promise(resolve => lateReads.push(()=>resolve(files[path])));
document.querySelector('.miku-refresh').click();
for (const fn of cleanup) fn();
check('closing the note releases every listener', listeners.size === 0);
for (const finish of lateReads) finish();
await settle();
check('a late read cannot recreate a closed view', document.querySelectorAll('.miku-stats').length === 0);
dv.io.load = originalLoad;
await mikuDashboard(dv, input); // Leave the real-data view visible in the screenshot.
document.getElementById('checks').textContent = JSON.stringify(checks);
document.getElementById('checks').style.display='none';
document.body.dataset.rendered = 'yes';
</script></html>'''
page_path = base / 'dashboard.html'
page_path.write_text(page)
env = dict(os.environ, XDG_CACHE_HOME=str(base / 'cache'))
command = ['chromium', '--headless', '--disable-gpu', '--no-first-run', '--no-default-browser-check', '--disable-background-networking', '--disable-extensions', '--disable-component-update', '--disable-sync', '--metrics-recording-only', '--disable-dev-shm-usage', '--host-resolver-rules=MAP * ~NOTFOUND', f'--user-data-dir={base / "profile"}', '--virtual-time-budget=3000', f'--window-size={args.width},1400', f'--screenshot={base / "dashboard.png"}', '--dump-dom', page_path.as_uri()]
result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=45)
(base / 'chromium.log').write_text(result.stderr)
(base / 'rendered.html').write_text(result.stdout)
assert result.returncode == 0, result.stderr
assert 'data-rendered="yes"' in result.stdout and 'data-error=' not in result.stdout, f'Dashboard failed; see {base}'
checks = json.loads(html.unescape(re.search(r'<pre id="checks"[^>]*>(.*?)</pre>', result.stdout, re.S).group(1)))
(base / 'results.json').write_text(json.dumps({'passed': True, 'width': args.width, 'checks': checks, 'count': len(checks)}, ensure_ascii=False, indent=2))
print(f'Dashboard browser checks passed ({len(checks)}):', base)
