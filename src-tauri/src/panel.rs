//! Read-only timer presentation for Waybar / the Windows notification area.
use crate::timer::{BackgroundMode, Database, Phase, Status};
use serde::Serialize;
use std::sync::{Arc, Condvar, Mutex, Weak};

pub const TRAY_ID: &str = "main";
const MAX_SUBSCRIBERS: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub enabled: bool,
    pub seconds: i64,
    pub text: String,
    pub tooltip: String,
    pub class: String,
}

#[derive(Serialize, serde::Deserialize)]
pub struct WaybarFrame {
    pub text: String,
    pub tooltip: String,
    pub class: String,
    pub alt: String,
}

fn clock(seconds: i64) -> String {
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

fn markup_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

impl View {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            seconds: 0,
            text: String::new(),
            tooltip: "Miku Focus — вывод в панель отключён".into(),
            class: "disabled".into(),
        }
    }

    pub fn capture(db: &Database, now: i64) -> Self {
        if db.settings.background_mode != BackgroundMode::Panel {
            return Self::disabled();
        }
        let seconds = db.remaining(now).max(0).saturating_add(999) / 1000;
        let time = clock(seconds);
        let (class, label, text) = match db.timer.status {
            Status::Focus => ("focus", "Работа", time.clone()),
            Status::Paused => ("paused", "Пауза", format!("Ⅱ {time}")),
            Status::Break if db.timer.phase == Phase::LongBreak => {
                ("break", "Длинный отдых", format!("Отдых {time}"))
            }
            Status::Break => ("break", "Отдых", format!("Отдых {time}")),
            Status::AwaitingDecision => ("decision", "Работа завершена", "Готово".into()),
            Status::Completed => ("completed", "Готово к следующей работе", time.clone()),
            Status::Idle => ("idle", "Готово к работе", time.clone()),
        };
        let category_id = db
            .timer
            .session
            .as_ref()
            .map(|s| &s.category_id)
            .unwrap_or(&db.selected_category);
        let category = db
            .categories
            .iter()
            .find(|c| &c.id == category_id)
            .map(|c| c.name.as_str())
            .unwrap_or(category_id);
        Self { enabled: true, seconds, text, class: class.into(), tooltip: format!("Miku Focus\n{label} · {time}\nКатегория: {category}\nНажмите, чтобы открыть главное окно") }
    }

    pub fn waybar(&self) -> WaybarFrame {
        WaybarFrame {
            text: self.text.clone(),
            tooltip: markup_escape(&self.tooltip),
            class: self.class.clone(),
            alt: self.class.clone(),
        }
    }
}

struct Pending {
    next: Option<Arc<View>>,
    closed: bool,
}
pub struct Subscription {
    pending: Mutex<Pending>,
    wake: Condvar,
}
impl Subscription {
    pub fn next(&self) -> Option<Arc<View>> {
        let mut pending = self.pending.lock().unwrap();
        while pending.next.is_none() && !pending.closed {
            pending = self.wake.wait(pending).unwrap();
        }
        if pending.closed {
            None
        } else {
            pending.next.take()
        }
    }
    pub fn close(&self) {
        self.pending.lock().unwrap().closed = true;
        self.wake.notify_all();
    }
}

struct HubState {
    current: Arc<View>,
    subscribers: Vec<Weak<Subscription>>,
    closed: bool,
}
pub struct Hub(Mutex<HubState>);
impl Hub {
    pub fn new() -> Self {
        Self(Mutex::new(HubState {
            current: Arc::new(View::disabled()),
            subscribers: Vec::new(),
            closed: false,
        }))
    }
    pub fn publish(&self, view: View) {
        let mut state = self.0.lock().unwrap();
        if state.closed || *state.current == view {
            return;
        }
        state.current = Arc::new(view);
        let current = state.current.clone();
        state.subscribers.retain(|weak| {
            let Some(slot) = weak.upgrade() else {
                return false;
            };
            let mut pending = slot.pending.lock().unwrap();
            if pending.closed {
                return false;
            }
            pending.next = Some(current.clone()); // latest wins: never block on a slow panel
            slot.wake.notify_one();
            true
        });
    }
    pub fn subscribe(&self) -> Option<Arc<Subscription>> {
        let mut state = self.0.lock().unwrap();
        state.subscribers.retain(|weak| {
            weak.upgrade()
                .is_some_and(|slot| !slot.pending.lock().unwrap().closed)
        });
        if state.closed || state.subscribers.len() >= MAX_SUBSCRIBERS {
            return None;
        }
        let slot = Arc::new(Subscription {
            pending: Mutex::new(Pending {
                next: Some(state.current.clone()),
                closed: false,
            }),
            wake: Condvar::new(),
        });
        state.subscribers.push(Arc::downgrade(&slot));
        Some(slot)
    }
    pub fn close(&self) {
        let mut state = self.0.lock().unwrap();
        state.closed = true;
        for slot in state.subscribers.iter().filter_map(Weak::upgrade) {
            slot.close();
        }
    }
}

pub fn module_config(executable: &std::path::Path) -> String {
    let quoted = format!("'{}'", executable.to_string_lossy().replace('\'', "'\\''"));
    serde_json::to_string_pretty(&serde_json::json!({ "custom/miku-focus": {
        "exec": format!("{quoted} --waybar"), "return-type": "json", "format": "{text}",
        "restart-interval": 3, "tooltip": true, "on-click": format!("{quoted} --show-main")
    } }))
    .unwrap()
}

pub struct Integration {
    pub hub: Arc<Hub>,
    pub config: String,
    pub error: Arc<Mutex<Option<String>>>,
    #[cfg(target_os = "linux")]
    server: Option<crate::panel_ipc::Server>,
}
impl Integration {
    pub fn new(
        root: &std::path::Path,
        show: impl Fn() -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        let hub = Arc::new(Hub::new());
        let error = Arc::new(Mutex::new(None));
        let config = std::env::current_exe()
            .map(|path| module_config(&path))
            .unwrap_or_default();
        #[cfg(target_os = "linux")]
        let server = match crate::panel_ipc::Server::start(root, hub.clone(), Arc::new(show)) {
            Ok(server) => Some(server),
            Err(message) => {
                *error.lock().unwrap() = Some(format!("Подключение панели недоступно: {message}"));
                None
            }
        };
        #[cfg(not(target_os = "linux"))]
        let _ = (root, show);
        Self {
            hub,
            config,
            error,
            #[cfg(target_os = "linux")]
            server,
        }
    }
    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }
}
impl Drop for Integration {
    fn drop(&mut self) {
        self.hub.close();
        #[cfg(target_os = "linux")]
        self.server.take();
    }
}

/// Original tiny bitmap digits; no font/image dependencies or changes to supplied assets.
#[cfg(any(windows, test))]
pub fn tray_pixels(view: &View) -> Vec<u8> {
    const DIGITS: [[u8; 5]; 10] = [
        [7, 5, 5, 5, 7],
        [2, 6, 2, 2, 7],
        [7, 1, 7, 4, 7],
        [7, 1, 7, 1, 7],
        [5, 5, 7, 1, 1],
        [7, 4, 7, 1, 7],
        [7, 4, 7, 5, 7],
        [7, 1, 1, 1, 1],
        [7, 5, 7, 5, 7],
        [7, 5, 7, 1, 7],
    ];
    let accent = match view.class.as_str() {
        "break" => [82, 150, 215, 255],
        "paused" => [217, 165, 72, 255],
        "decision" => [226, 100, 128, 255],
        _ => [64, 190, 162, 255],
    };
    let mut pixels = vec![0; 32 * 32 * 4];
    for y in 1..31 {
        for x in 1..31 {
            let color = if x <= 2 || x >= 29 || y <= 2 || y >= 29 {
                accent
            } else {
                [12, 30, 35, 255]
            };
            pixels[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4].copy_from_slice(&color);
        }
    }
    for (text, top) in [
        (format!("{:02}", (view.seconds / 60).min(999)), 4),
        (format!("{:02}", view.seconds % 60), 18),
    ] {
        let left = (32 - (text.len() * 8 - 2)) / 2;
        for (i, digit) in text.bytes().enumerate() {
            for (row, mask) in DIGITS[(digit - b'0') as usize].iter().enumerate() {
                for column in 0..3 {
                    if mask & (1 << (2 - column)) != 0 {
                        for dy in 0..2 {
                            for dx in 0..2 {
                                let offset =
                                    ((top + row * 2 + dy) * 32 + left + i * 8 + column * 2 + dx)
                                        * 4;
                                pixels[offset..offset + 4].copy_from_slice(&[245, 255, 252, 255]);
                            }
                        }
                    }
                }
            }
        }
    }
    pixels
}

#[cfg(any(windows, test))]
#[cfg_attr(test, allow(dead_code))]
pub fn start_native(
    app: tauri::AppHandle,
    hub: Arc<Hub>,
    error: Arc<Mutex<Option<String>>>,
    status: tauri::menu::MenuItem<tauri::Wry>,
) {
    use tauri::Emitter;
    let Some(subscription) = hub.subscribe() else {
        return;
    };
    std::thread::spawn(move || {
        while let Some(view) = subscription.next() {
            let result = (|| -> Result<(), String> {
                let tray = app
                    .tray_by_id(TRAY_ID)
                    .ok_or("Значок области уведомлений недоступен")?;
                if view.enabled {
                    tray.set_icon(Some(tauri::image::Image::new_owned(
                        tray_pixels(&view),
                        32,
                        32,
                    )))
                    .map_err(|e| e.to_string())?;
                    let mut tooltip = String::new();
                    for c in view.tooltip.chars() {
                        if tooltip.encode_utf16().count() + c.len_utf16() > 127 {
                            break;
                        }
                        tooltip.push(c);
                    }
                    tray.set_tooltip(Some(tooltip)).map_err(|e| e.to_string())?;
                    status
                        .set_text(view.tooltip.lines().nth(1).unwrap_or("Таймер"))
                        .map_err(|e| e.to_string())?;
                } else {
                    tray.set_icon(app.default_window_icon().cloned())
                        .map_err(|e| e.to_string())?;
                    tray.set_tooltip(Some("Miku Focus"))
                        .map_err(|e| e.to_string())?;
                    status
                        .set_text("Таймер в трее отключён")
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            })();
            match result {
                Err(message) => {
                    let mut previous = error.lock().unwrap();
                    if previous.as_ref() != Some(&message) {
                        *previous = Some(message.clone());
                        let _ = app.emit("panel-error", Some(message));
                    }
                }
                Ok(()) => {
                    if error.lock().unwrap().take().is_some() {
                        let _ = app.emit("panel-error", Option::<String>::None);
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn active() -> Database {
        let mut db = Database::default();
        db.settings.background_mode = BackgroundMode::Panel;
        db.settings.focus_minutes = 1;
        db.action("start", 1000).unwrap();
        db
    }
    #[test]
    fn rust_deadline_is_the_only_clock_and_capture_does_not_mutate_accounting() {
        let db = active();
        let before = serde_json::to_value(&db).unwrap();
        assert_eq!(View::capture(&db, 1001).text, "01:00");
        assert_eq!(View::capture(&db, 2000).text, "00:59");
        assert_eq!(View::capture(&db, 999999).text, "00:00");
        assert_eq!(serde_json::to_value(db).unwrap(), before);
    }
    #[test]
    fn pause_decision_and_break_are_distinct_and_never_show_the_completion_question() {
        let mut db = active();
        db.action("pause", 11000).unwrap();
        assert_eq!(View::capture(&db, 51000).text, "Ⅱ 00:50");
        db.action("resume", 51000).unwrap();
        db.tick(101000);
        let view = View::capture(&db, 999999);
        assert_eq!(view.class, "decision");
        assert_eq!(view.text, "Готово");
        assert!(!view.tooltip.contains("пять минут"));
        db.action("finish", 999999).unwrap();
        assert_eq!(View::capture(&db, 999999).text, "Отдых 05:00");
    }
    #[test]
    fn disabled_mode_is_constant_even_when_the_timer_runs() {
        let mut db = active();
        db.settings.background_mode = BackgroundMode::Companion;
        assert_eq!(View::capture(&db, 1001), View::capture(&db, 999999));
        assert_eq!(View::capture(&db, 1001).waybar().text, "");
    }
    #[test]
    fn tooltip_escapes_user_markup_and_uses_the_started_session_category() {
        let mut db = active();
        db.categories[2].name = "<b>Код & тест</b>".into();
        db.selected_category = "reading".into();
        let view = View::capture(&db, 1000);
        assert!(view
            .waybar()
            .tooltip
            .contains("&lt;b&gt;Код &amp; тест&lt;/b&gt;"));
        assert!(!view.waybar().tooltip.contains("Чтение"));
    }
    #[test]
    fn a_slow_panel_has_only_one_latest_pending_view_and_disconnect_releases_capacity() {
        let hub = Hub::new();
        let sub = hub.subscribe().unwrap();
        let db = active();
        for now in 1000..21000 {
            hub.publish(View::capture(&db, now));
        }
        assert_eq!(sub.next().unwrap().text, "00:41");
        sub.close();
        let slots: Vec<_> = (0..MAX_SUBSCRIBERS)
            .map(|_| hub.subscribe().unwrap())
            .collect();
        assert!(hub.subscribe().is_none());
        slots[0].close();
        assert!(hub.subscribe().is_some());
        hub.close();
        assert!(slots[1].next().is_none());
    }
    #[test]
    fn tray_bitmap_is_bounded_original_and_changes_with_seconds_and_phase() {
        let db = active();
        let first = View::capture(&db, 1000);
        let pixels = tray_pixels(&first);
        assert_eq!(pixels.len(), 32 * 32 * 4);
        assert!(pixels.chunks(4).any(|p| p == [245, 255, 252, 255]));
        assert_ne!(pixels, tray_pixels(&View::capture(&db, 2000)));
        let mut paused = first;
        paused.class = "paused".into();
        assert_ne!(pixels, tray_pixels(&paused));
    }
    #[test]
    fn generated_waybar_commands_quote_paths_and_do_not_edit_configuration() {
        let config: serde_json::Value =
            serde_json::from_str(&module_config(std::path::Path::new("/home/a'b/Miku Focus")))
                .unwrap();
        assert_eq!(
            config["custom/miku-focus"]["exec"],
            "'/home/a'\\''b/Miku Focus' --waybar"
        );
        assert_eq!(
            config["custom/miku-focus"]["on-click"],
            "'/home/a'\\''b/Miku Focus' --show-main"
        );
        assert_eq!(config["custom/miku-focus"]["return-type"], "json");
    }
    #[test]
    fn background_mode_defaults_for_legacy_data_and_patch_preserves_the_timer() {
        let mut db = active();
        let timer = serde_json::to_value(&db.timer).unwrap();
        let patch =
            serde_json::from_value(serde_json::json!({"backgroundMode":"Companion"})).unwrap();
        db.update_settings(patch).unwrap();
        assert_eq!(serde_json::to_value(&db.timer).unwrap(), timer);
        assert_eq!(
            serde_json::from_value::<crate::timer::Settings>(serde_json::json!({}))
                .unwrap()
                .background_mode,
            BackgroundMode::Companion
        );
        assert!(serde_json::from_value::<crate::timer::SettingsPatch>(
            serde_json::json!({"backgroundMode":"Unknown"})
        )
        .is_err());
    }
}
