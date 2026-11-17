mod audio;
mod dialogues;
mod idle;
mod platform;
mod store;
mod timer;
mod window_geometry;

use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    sync::{Condvar, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};
use timer::{Category, Database, SettingsPatch};

#[derive(Clone, Serialize)]
struct ReactionEvent {
    id: u64,
    event: String,
}
struct Runtime {
    db: Database,
    active_window: String,
    reactions: VecDeque<ReactionEvent>,
    next_id: u64,
    recovered: bool,
    system_idle: bool,
    idle_adapter: String,
    error: Option<String>,
    last_initiative: i64,
    last_goal: i64,
    quitting: bool,
    quit_generation: u64,
}
impl Runtime {
    fn react(&mut self, event: &str) {
        self.next_id += 1;
        if self.reactions.len() >= 16 {
            self.reactions.pop_front();
        }
        self.reactions.push_back(ReactionEvent {
            id: self.next_id,
            event: event.into(),
        });
    }
    fn initiative(&mut self, event: &str, now: i64) {
        let interval = match self.db.settings.talkativeness.as_str() {
            "Disabled" => return,
            "Quiet" => 60,
            "Chatty" => 10,
            _ => 25,
        } * 60_000;
        if now - self.last_initiative >= interval {
            self.last_initiative = now;
            self.react(event);
        }
    }
}
struct AppState {
    drag: Mutex<Option<platform::DragOrigin>>,
    resize: Mutex<Option<platform::ResizeOrigin>>,
    runtime: Mutex<Runtime>,
    wake: Condvar,
    store: store::SettingsStore,
    epoch: i64,
    clock: Instant,
}
impl AppState {
    fn now(&self) -> i64 {
        self.epoch + self.clock.elapsed().as_millis() as i64
    }
    fn save(&self, runtime: &mut Runtime) -> Result<(), String> {
        let result = self.store.save(&runtime.db.checkpoint(self.now()));
        runtime.error = result.as_ref().err().cloned();
        result
    }
}

fn snapshot_value(state: &AppState, r: &Runtime) -> Value {
    json!({
        "now": state.now(), "remainingMs": r.db.remaining(state.now()), "timer": r.db.timer,
        "settings": r.db.settings, "categories": r.db.categories, "selectedCategory": r.db.selected_category,
        "sessions": r.db.sessions, "breaks": r.db.breaks, "activeWindow": r.active_window,
        "reactions": r.reactions, "recovered": r.recovered, "systemIdle": r.system_idle,
        "idleAdapter": r.idle_adapter, "platform": if platform::is_hyprland() { "Hyprland / Wayland" } else { std::env::consts::OS },
        "storagePath": state.store.root, "error": r.error,
        "rendererCompatibility": std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").as_deref() == Ok("1"),
        "quitting": r.quitting,
    })
}
fn publish(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let r = state.runtime.lock().unwrap();
    // Only the visible UI receives clock messages, full snapshots only on changes.
    let _ = app.emit("state", snapshot_value(&state, &r));
}

#[tauri::command]
fn get_state(state: tauri::State<AppState>) -> Value {
    snapshot_value(&state, &state.runtime.lock().unwrap())
}
#[tauri::command]
fn timer_action(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    action: String,
) -> Result<(), String> {
    let mut r = state.runtime.lock().unwrap();
    let old_best = r.db.sessions.iter().map(|s| s.work_ms()).max().unwrap_or(0);
    let deadline_event = r.db.tick(state.now());
    let result = r.db.action(&action, state.now());
    let event = result.as_ref().ok().copied().flatten().or(deadline_event);
    if let Some(event) = event {
        r.react(event);
    }
    if result.is_ok()
        && action == "finish"
        && r.db
            .sessions
            .last()
            .is_some_and(|s| s.work_ms() > old_best && old_best > 0)
    {
        r.react("record");
    }
    let saved = state.save(&mut r);
    state.wake.notify_one();
    drop(r);
    publish(&app);
    saved?;
    result.map(|_| ())
}
#[tauri::command]
fn claim_reaction(state: tauri::State<AppState>, window: tauri::WebviewWindow, id: u64) -> bool {
    let mut r = state.runtime.lock().unwrap();
    if r.active_window != window.label() {
        return false;
    }
    if let Some(index) = r.reactions.iter().position(|e| e.id == id) {
        r.reactions.remove(index);
        true
    } else {
        false
    }
}
#[tauri::command]
fn request_reaction(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    event: String,
) -> Result<(), String> {
    let mut r = state.runtime.lock().unwrap();
    match event.as_str() {
        "touch" => {
            let event = if r.db.timer.phase == timer::Phase::Focus {
                "touch"
            } else {
                "break_touch"
            };
            r.react(event);
        }
        "goal" if state.now() - r.last_goal > 20 * 3600_000 => {
            r.last_goal = state.now();
            r.react("goal");
        }
        "goal" => {}
        _ => return Err("Неподдерживаемая реакция".into()),
    }
    drop(r);
    publish(&app);
    Ok(())
}
#[tauri::command]
fn update_settings(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    patch: SettingsPatch,
) -> Result<(), String> {
    let mut r = state.runtime.lock().unwrap();
    let previous_top = r.db.settings.always_on_top;
    r.db.update_settings(patch)?;
    let top_changed = previous_top != r.db.settings.always_on_top;
    let active = r.active_window.clone();
    let top = r.db.settings.always_on_top;
    state.save(&mut r)?;
    drop(r);
    if top_changed && active == "companion" {
        let size = platform::size(&app).unwrap_or(window_geometry::DEFAULT_SIZE);
        platform::prepare(&app, "companion", top, platform::position(&app), size)?;
    }
    publish(&app);
    Ok(())
}
#[tauri::command]
fn set_focus_minutes(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    minutes: u32,
) -> Result<(), String> {
    let mut r = state.runtime.lock().unwrap();
    r.db.set_focus_minutes(minutes)?;
    state.save(&mut r)?;
    drop(r);
    publish(&app);
    Ok(())
}
#[tauri::command]
fn adjust_focus_minutes(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    delta: i32,
) -> Result<(), String> {
    let mut r = state.runtime.lock().unwrap();
    r.db.adjust_focus_minutes(delta)?;
    state.save(&mut r)?;
    drop(r);
    publish(&app);
    Ok(())
}
#[tauri::command]
fn select_category(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    id: String,
) -> Result<(), String> {
    let mut r = state.runtime.lock().unwrap();
    if !r.db.categories.iter().any(|c| c.id == id && !c.archived) {
        return Err("Категория недоступна".into());
    }
    r.db.selected_category = id;
    state.save(&mut r)?;
    drop(r);
    publish(&app);
    Ok(())
}
#[tauri::command]
fn save_category(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    mut category: Category,
) -> Result<(), String> {
    category.name = category.name.trim().into();
    if category.name.is_empty()
        || category.name.chars().count() > 60
        || category.color.len() != 7
        || !category.color.starts_with('#')
        || !category.color[1..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Неверное имя или цвет категории".into());
    }
    let mut r = state.runtime.lock().unwrap();
    let mut categories = r.db.categories.clone();
    if let Some(c) = categories.iter_mut().find(|c| c.id == category.id) {
        *c = category;
    } else {
        if r.db.categories.len() >= 200 {
            return Err("Максимум 200 категорий".into());
        }
        category.id = format!("custom-{}-{}", state.now(), categories.len());
        categories.push(category);
    }
    if !categories
        .iter()
        .any(|c| c.id == r.db.selected_category && !c.archived)
    {
        r.db.selected_category = categories
            .iter()
            .find(|c| !c.archived)
            .ok_or("Оставьте хотя бы одну активную категорию")?
            .id
            .clone();
    }
    r.db.categories = categories;
    state.save(&mut r)?;
    drop(r);
    publish(&app);
    Ok(())
}

#[tauri::command]
fn import_asset(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    request: tauri::ipc::Request<'_>,
) -> Result<String, String> {
    let kind = request
        .headers()
        .get("x-miku-kind")
        .and_then(|v| v.to_str().ok())
        .ok_or("Нет типа ресурса")?
        .to_string();
    let id = request
        .headers()
        .get("x-miku-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Нужны бинарные данные ресурса".into());
    };
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("Один ресурс не должен превышать 20 МиБ".into());
    }
    let png = bytes.starts_with(b"\x89PNG\r\n\x1a\n");
    if png {
        if bytes.len() < 24 {
            return Err("Повреждённый PNG".into());
        }
        let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
        let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err("Изображение должно быть не больше 4096×4096".into());
        }
    }
    let jpeg = bytes.starts_with(b"\xff\xd8\xff");
    let ogg = bytes.starts_with(b"OggS");
    let wav = bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE");
    let emotions = [
        "Neutral",
        "Happy",
        "Calm",
        "Pleased",
        "Surprised",
        "Relief",
        "Worried",
        "Dissatisfied",
        "Angry",
        "Sad",
    ];
    let ext = match kind.as_str() {
        "background" if png || jpeg => {
            if png {
                "png"
            } else {
                "jpg"
            }
        }
        "sprite" if png && emotions.contains(&id.as_str()) => "png",
        "voice" | "sfx" if (ogg || wav) && audio::valid_id(&id) => {
            if ogg {
                "ogg"
            } else {
                "wav"
            }
        }
        _ => {
            return Err(
                "Нужен PNG/JPEG для фона, PNG для эмоции или WAV/OGG для голоса и эффекта".into(),
            )
        }
    };
    // Keep the capacity check and insert under one lock, including concurrent WebView imports.
    let mut r = state.runtime.lock().unwrap();
    if kind == "voice" || kind == "sfx" {
        let sounds = if kind == "voice" {
            &r.db.settings.voices
        } else {
            &r.db.settings.sfx
        };
        if sounds.len() >= 200 && !sounds.contains_key(&id) {
            return Err("Максимум 200 импортированных звуков каждого типа".into());
        }
    }
    // Unique filenames: do not overwrite resources currently being read by a webview.
    static IMPORT_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = IMPORT_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = format!(
        "{kind}-{}-{}-{sequence}.{}",
        if kind == "background" { "scene" } else { &id },
        state.now(),
        ext
    );
    let path = state.store.root.join("assets").join(name);
    store::atomic_write(&path, &bytes)?;
    let path = path.to_string_lossy().into_owned();
    match kind.as_str() {
        "background" => r.db.settings.background = Some(path.clone()),
        "sprite" => {
            r.db.settings.sprites.insert(id, path.clone());
        }
        "sfx" => {
            r.db.settings.sfx.insert(id, path.clone());
        }
        _ => {
            r.db.settings.voices.insert(id, path.clone());
        }
    }
    state.save(&mut r)?;
    drop(r);
    publish(&app);
    Ok(path)
}
#[tauri::command]
fn reset_art(app: tauri::AppHandle, state: tauri::State<AppState>) -> Result<(), String> {
    let mut r = state.runtime.lock().unwrap();
    r.db.settings.background = None;
    r.db.settings.sprites.clear();
    state.save(&mut r)?;
    drop(r);
    publish(&app);
    Ok(())
}
#[tauri::command]
fn bind_sfx(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    event: String,
    sound: Option<String>,
) -> Result<(), String> {
    let mut r = state.runtime.lock().unwrap();
    r.db.settings.bind_sfx(&event, sound)?;
    state.save(&mut r)?;
    drop(r);
    publish(&app);
    Ok(())
}
#[tauri::command]
fn clear_sound(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    kind: String,
    id: String,
) -> Result<(), String> {
    let mut r = state.runtime.lock().unwrap();
    r.db.settings.clear_sound(&kind, &id)?;
    state.save(&mut r)?;
    drop(r);
    publish(&app);
    Ok(())
}
#[tauri::command]
fn get_dialogues(state: tauri::State<AppState>) -> Result<Value, String> {
    let path = state.store.root.join("dialogues.json");
    let text = if path.exists() {
        std::fs::read_to_string(path).map_err(|e| e.to_string())?
    } else {
        include_str!("../../public/dialogues.json").into()
    };
    serde_json::from_str(&text)
        .map(dialogues::with_rest_dialogues)
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn save_dialogues(state: tauri::State<AppState>, data: Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 || data["version"] != 1 || !data["dialogues"].is_array() {
        return Err("Неверный JSON диалогов".into());
    }
    store::atomic_write(&state.store.root.join("dialogues.json"), &bytes)
}
fn csv_cell(text: &str) -> String {
    let prefix = if text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        "'"
    } else {
        ""
    };
    format!("\"{prefix}{}\"", text.replace('"', "\"\""))
}
#[tauri::command]
fn export_data(state: tauri::State<AppState>, format: String) -> Result<String, String> {
    let r = state.runtime.lock().unwrap();
    let bytes = if format == "json" {
        serde_json::to_vec_pretty(&r.db.checkpoint(state.now())).map_err(|e| e.to_string())?
    } else if format == "csv" {
        let mut rows =
            vec!["session_id,category,start_ms,end_ms,work_ms,extensions,completed".to_string()];
        let mut sessions = r.db.sessions.clone();
        if let Some(mut current) = r.db.timer.session.clone() {
            if let Some(start) = r.db.timer.segment_started_at {
                if r.db.timer.phase == timer::Phase::Focus {
                    current.segments.push(timer::Segment {
                        start_ms: start,
                        end_ms: state
                            .now()
                            .min(r.db.timer.deadline_ms.unwrap_or(state.now())),
                    });
                }
            }
            sessions.push(current);
        }
        for s in &sessions {
            let category =
                r.db.categories
                    .iter()
                    .find(|c| c.id == s.category_id)
                    .map(|c| c.name.as_str())
                    .unwrap_or(&s.category_id);
            rows.push(format!(
                "{},{},{},{},{},{},{}",
                csv_cell(&s.id),
                csv_cell(category),
                s.started_at,
                s.ended_at.map(|e| e.to_string()).unwrap_or_default(),
                s.work_ms(),
                s.extensions,
                s.completed
            ));
        }
        format!("\u{feff}{}\r\n", rows.join("\r\n")).into_bytes()
    } else {
        return Err("Нужен csv или json".into());
    };
    let dir = state.store.root.join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("miku-focus-{}.{}", state.now(), format));
    store::atomic_write(&path, &bytes)?;
    Ok(path.to_string_lossy().into_owned())
}

fn switch_window(app: &tauri::AppHandle, action: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    if action == "quit" {
        let pos = platform::position(app);
        let size = platform::size(app);
        let mut r = state.runtime.lock().unwrap();
        if r.quitting {
            return Ok(());
        }
        if r.active_window == "companion" {
            if pos.is_some() {
                r.db.settings.companion_position = pos;
            }
            if let Some(size) = size {
                r.db.settings.companion_size = Some(window_geometry::bounded_size(size));
            }
        }
        state.save(&mut r)?;
        r.quitting = true;
        r.quit_generation += 1;
        let generation = r.quit_generation;
        drop(r);
        publish(app);
        let handle = app.clone();
        // One bounded fail-safe if the active WebView cannot finish its presentation.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(6));
            if let Err(error) = finish_quit(&handle, Some(generation)) {
                eprintln!("{error}");
            }
        });
        return Ok(());
    }
    if !["main", "companion"].contains(&action) {
        return Err("Неизвестный режим окна".into());
    }
    if state.runtime.lock().unwrap().quitting {
        return Err("Приложение завершает работу".into());
    }
    let position = platform::position(app);
    let current_size = platform::size(app);
    let (top, pos, size) = {
        let mut r = state.runtime.lock().unwrap();
        if r.active_window == "companion" {
            if position.is_some() {
                r.db.settings.companion_position = position;
            }
            if let Some(size) = current_size {
                r.db.settings.companion_size = Some(window_geometry::bounded_size(size));
            }
        }
        r.active_window = action.into();
        state.save(&mut r)?;
        (
            r.db.settings.always_on_top,
            r.db.settings.companion_position,
            window_geometry::effective_size(r.db.settings.companion_size),
        )
    };
    let other = if action == "main" {
        "companion"
    } else {
        "main"
    };
    app.get_webview_window(action)
        .ok_or("Окно недоступно")?
        .show()
        .map_err(|e| e.to_string())?;
    app.get_webview_window(other)
        .ok_or("Окно недоступно")?
        .hide()
        .map_err(|e| e.to_string())?;
    if action == "main" {
        let _ = app.get_webview_window(action).unwrap().set_focus();
    }
    let handle = app.clone();
    let label = action.to_string();
    // Let the compositor map the surface before addressing it. One delayed action, no polling.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        if let Err(e) = platform::prepare(&handle, &label, top, pos, size) {
            handle.state::<AppState>().runtime.lock().unwrap().error = Some(e);
            publish(&handle);
        }
    });
    publish(app);
    Ok(())
}
#[tauri::command]
fn window_action(app: tauri::AppHandle, action: String) -> Result<(), String> {
    switch_window(&app, &action)
}

fn finish_quit(app: &tauri::AppHandle, generation: Option<u64>) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut r = state.runtime.lock().unwrap();
    if !r.quitting || generation.is_some_and(|value| value != r.quit_generation) {
        return Ok(());
    }
    if let Err(error) = state.save(&mut r) {
        r.quitting = false;
        drop(r);
        publish(app);
        return Err(error);
    }
    drop(r);
    app.exit(0);
    Ok(())
}
#[tauri::command]
fn complete_quit(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    if state.runtime.lock().unwrap().active_window != window.label() {
        return Err("Выход подтверждает только активное окно".into());
    }
    finish_quit(&app, None)
}
#[tauri::command]
fn cancel_quit(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    let mut r = state.runtime.lock().unwrap();
    if r.active_window != window.label() {
        return Err("Выход отменяет только активное окно".into());
    }
    r.quitting = false;
    drop(r);
    publish(&app);
    Ok(())
}

#[tauri::command]
fn drag_companion(
    state: tauri::State<AppState>,
    window: tauri::WebviewWindow,
    phase: String,
) -> Result<(), String> {
    if window.label() != "companion" || !platform::is_hyprland() {
        return Err("Drag adapter is companion-only on Hyprland".into());
    }
    let mut drag = state.drag.lock().unwrap();
    match phase.as_str() {
        "start" => *drag = Some(platform::begin_drag()?),
        "move" => {
            if let Some(d) = drag.as_mut() {
                platform::move_drag(d)?;
            }
        }
        "end" => {
            if let Some(d) = drag.take() {
                let mut r = state.runtime.lock().unwrap();
                r.db.settings.companion_position = Some(d.last);
                state.save(&mut r)?;
            }
        }
        _ => return Err("Invalid drag phase".into()),
    }
    Ok(())
}

#[tauri::command]
fn resize_companion(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    window: tauri::WebviewWindow,
    phase: String,
) -> Result<(), String> {
    if window.label() != "companion" || !platform::is_hyprland() {
        return Err(
            "Изменение размера этим адаптером доступно только помощнику на Hyprland".into(),
        );
    }
    let mut resize = state.resize.lock().unwrap();
    match phase.as_str() {
        "start" => *resize = Some(platform::begin_resize()?),
        "move" => {
            if let Some(resize) = resize.as_mut() {
                platform::move_resize(resize)?;
            }
        }
        "end" => {
            if let Some(mut resize) = resize.take() {
                platform::move_resize(&mut resize)?;
                let mut r = state.runtime.lock().unwrap();
                r.db.settings.companion_size = Some(resize.last);
                state.save(&mut r)?;
                drop(r);
                publish(&app);
            }
        }
        _ => return Err("Неизвестная фаза изменения размера".into()),
    }
    Ok(())
}

#[tauri::command]
fn adjust_companion_size(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    window: tauri::WebviewWindow,
    delta_width: i32,
    delta_height: i32,
) -> Result<(), String> {
    if window.label() != "companion" {
        return Err("Размер можно менять только у помощника".into());
    }
    let current = platform::size(&app).ok_or("Не удалось прочитать размер помощника")?;
    let size = window_geometry::resized_size(current, (delta_width, delta_height));
    platform::resize_to(&app, size)?;
    let mut r = state.runtime.lock().unwrap();
    r.db.settings.companion_size = Some(size);
    state.save(&mut r)?;
    drop(r);
    publish(&app);
    Ok(())
}

#[tauri::command]
fn save_companion_geometry(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    if window.label() != "companion" {
        return Err("Геометрия доступна только помощнику".into());
    }
    let position = platform::position(&app);
    let size = platform::size(&app).map(window_geometry::bounded_size);
    let mut r = state.runtime.lock().unwrap();
    let mut changed = false;
    if let Some(position) = position {
        if r.db.settings.companion_position != Some(position) {
            r.db.settings.companion_position = Some(position);
            changed = true;
        }
    }
    if let Some(size) = size {
        if r.db.settings.companion_size != Some(size) {
            r.db.settings.companion_size = Some(size);
            changed = true;
        }
    }
    if changed {
        state.save(&mut r)?;
    }
    drop(r);
    if changed {
        publish(&app);
    }
    Ok(())
}

pub fn run() {
    // GTK's own file chooser/menu translations, scoped to this process only.
    #[cfg(target_os = "linux")]
    std::env::set_var("LANGUAGE", "ru");
    platform::configure_graphics();
    tauri::Builder::default()
        .setup(|app| {
            let root = app.path().app_data_dir()?;
            let store = store::SettingsStore::new(root).map_err(std::io::Error::other)?;
            let mut db = store.load().map_err(std::io::Error::other)?;
            let epoch = (SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64).max(db.saved_at);
            let recovered = db.recover(epoch);
            let mut runtime = Runtime { db, active_window: "main".into(), reactions: VecDeque::new(), next_id: 0,
                recovered, system_idle: false, idle_adapter: "connecting".into(), error: None, last_initiative: epoch, last_goal: 0, quitting: false, quit_generation: 0 };
            let launch_event = if runtime.db.timer.phase == timer::Phase::Focus { "launch" } else { "break_start" };
            runtime.react(launch_event);
            app.manage(AppState { drag: Mutex::new(None), resize: Mutex::new(None), runtime: Mutex::new(runtime), wake: Condvar::new(), store, epoch, clock: Instant::now() });
            let state = app.state::<AppState>(); state.save(&mut state.runtime.lock().unwrap()).map_err(std::io::Error::other)?;

            use tauri::menu::{Menu, MenuItem};
            let show = MenuItem::with_id(app, "main", "Открыть Miku Focus", true, None::<&str>)?;
            let companion = MenuItem::with_id(app, "companion", "Компактный помощник", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Сохранить и выйти", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &companion, &quit])?;
            let mut tray = tauri::tray::TrayIconBuilder::new().tooltip("Miku Focus").menu(&menu)
                .on_menu_event(|app, event| { if let Err(e) = switch_window(app, event.id.as_ref()) { eprintln!("{e}"); } });
            if let Some(icon) = app.default_window_icon() { tray = tray.icon(icon.clone()); }
            tray.build(app)?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut last_checkpoint = 0;
                loop {
                    let state = handle.state::<AppState>();
                    let mut r = state.runtime.lock().unwrap(); let now = state.now();
                    let event = r.db.tick(now);
                    if let Some(event) = event { r.react(event); }
                    let before = r.next_id;
                    let long_work = r.db.timer.session.as_ref().is_some_and(|s| s.work_ms() + r.db.timer.segment_started_at.map(|t| now - t).unwrap_or(0) >= 60 * 60_000);
                    if !r.system_idle { r.initiative(if long_work { "long_work" } else { "company" }, now); }
                    if (r.db.running() && now - last_checkpoint >= 30_000) || event.is_some() { let _ = state.save(&mut r); last_checkpoint = now; }
                    let running = r.db.running(); let active = r.active_window.clone();
                    if running { let _ = handle.emit_to(&active, "clock", json!({ "now": now, "remainingMs": r.db.remaining(now), "timer": r.db.timer })); }
                    if event.is_some() || before != r.next_id { let _ = handle.emit("state", snapshot_value(&state, &r)); }
                    let timeout = if running { 1 } else { 30 };
                    let _guard = state.wake.wait_timeout(r, Duration::from_secs(timeout)).unwrap();
                }
            });
            idle::start(app.handle().clone()); platform::watch_focus(app.handle().clone());
            let mode = if std::env::args().any(|a| a == "--companion") { "companion" } else { "main" };
            switch_window(app.handle(), mode).map_err(std::io::Error::other)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let action = if window.label() == "main" { "companion" } else { "main" };
                if let Err(e) = switch_window(window.app_handle(), action) { eprintln!("{e}"); }
            }
        })
        .invoke_handler(tauri::generate_handler![get_state, timer_action, claim_reaction, request_reaction,
            update_settings, set_focus_minutes, adjust_focus_minutes, select_category, save_category, import_asset, reset_art, bind_sfx, clear_sound, get_dialogues, save_dialogues,
            export_data, window_action, complete_quit, cancel_quit, drag_companion, resize_companion, adjust_companion_size, save_companion_geometry])
        .run(tauri::generate_context!())
        .expect("Miku Focus startup failed; user data has not been discarded");
}
