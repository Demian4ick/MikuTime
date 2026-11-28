import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('no WebView can invoke a command before application state is registered', () => {
  const config = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
  assert.deepEqual(config.app.windows.map((window: { label: string }) => window.label).sort(), ['companion', 'main']);
  for (const window of config.app.windows) assert.equal(window.create, false, window.label);
  const rust = readFileSync(new URL('../src-tauri/src/lib.rs', import.meta.url), 'utf8');
  const setup = rust.slice(rust.indexOf('.setup(|app|'));
  const manage = setup.indexOf('app.manage(AppState');
  const create = setup.indexOf('tauri::WebviewWindowBuilder::from_config');
  assert.ok(manage >= 0 && create > manage);
  assert.ok(setup.indexOf('state.save(') < create);
  assert.match(setup.slice(create, setup.indexOf('use tauri::menu', create)), /\.visible\(false\)[\s\S]*\.build\(\)/);
  assert.ok(setup.lastIndexOf('switch_window(app.handle(), mode)') > create);
});

test('installed startup errors never tell users to run a development server', () => {
  const app = readFileSync(new URL('../src/App.svelte', import.meta.url), 'utf8');
  const fatal = app.slice(app.indexOf('{#if fatal}<section'), app.indexOf('{:else if !snapshot}'));
  assert.match(fatal, /Данные не удалены/);
  assert.doesNotMatch(fatal, /npm run|tauri dev|не браузер/);
});

test('empty category submission points to the name without disabling the explanation', () => {
  const settings = readFileSync(new URL('../src/components/Settings.svelte', import.meta.url), 'utf8');
  const form = settings.slice(settings.indexOf('<form class="category-editor"'), settings.indexOf('</form>') + 7);
  assert.match(form, /novalidate/);
  assert.match(form, /aria-invalid=\{nameMissing\}/);
  assert.match(form, /aria-describedby="category-name-hint"/);
  assert.doesNotMatch(form, /disabled=\{[^}]*name\.trim/);
  assert.match(settings, /categoryNameInput\?\.focus\(\)/);
  assert.match(settings, /Сначала придумайте название категории/);
  assert.match(settings, /name-hint-pulse 1s ease-in-out 2/);
  assert.match(settings, /prefers-reduced-motion: reduce/);
  assert.doesNotMatch(settings, /infinite/);
});
