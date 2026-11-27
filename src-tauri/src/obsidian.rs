//! One-way statistics snapshots. Vault I/O never holds the timer/runtime mutex.
use crate::{
    store,
    timer::{Category, Database, Phase, Segment, Session, Status},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const FORMAT: &str = "miku-focus-statistics";
const SCHEMA: u32 = 1;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Config {
    schema_version: u32,
    source_id: String,
    folder: Option<PathBuf>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportStatus {
    pub folder: Option<PathBuf>,
    pub file: Option<PathBuf>,
    pub source_os: String,
    pub source_id: String,
    pub exporting: bool,
    pub last_exported_at: Option<i64>,
    pub error: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Totals {
    work_ms: i64,
    break_ms: i64,
    completed_sessions: usize,
    extensions: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Content {
    categories: Vec<Category>,
    sessions: Vec<Session>,
    breaks: Vec<Segment>,
    current_session_id: Option<String>,
    timer_status: Status,
    timer_phase: Phase,
    daily_goal_minutes: u32,
    totals: Totals,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Statistics {
    captured_at: i64,
    #[serde(flatten)]
    content: Content,
}

impl Statistics {
    fn capture(db: &Database) -> Self {
        let now = db.saved_at;
        let mut sessions = db.sessions.clone();
        if let Some(mut current) = db.timer.session.clone() {
            if db.timer.status == Status::Focus && db.timer.phase == Phase::Focus {
                if let Some(start) = db.timer.segment_started_at {
                    let end = now.min(db.timer.deadline_ms.unwrap_or(now));
                    if end > start {
                        current.segments.push(Segment {
                            start_ms: start,
                            end_ms: end,
                        });
                    }
                }
            }
            sessions.push(current);
        }
        let mut breaks = db.breaks.clone();
        if db.timer.status == Status::Break {
            if let Some(start) = db.timer.segment_started_at {
                let end = now.min(db.timer.deadline_ms.unwrap_or(now));
                if end > start {
                    breaks.push(Segment {
                        start_ms: start,
                        end_ms: end,
                    });
                }
            }
        }
        let totals = Totals {
            work_ms: sessions.iter().map(Session::work_ms).sum(),
            break_ms: breaks.iter().map(Segment::duration).sum(),
            completed_sessions: sessions
                .iter()
                .filter(|s| s.completed && s.ended_at.is_some())
                .count(),
            extensions: sessions.iter().map(|s| u64::from(s.extensions)).sum(),
        };
        Self {
            captured_at: now,
            content: Content {
                categories: db.categories.clone(),
                sessions,
                breaks,
                current_session_id: db.timer.session.as_ref().map(|s| s.id.clone()),
                timer_status: db.timer.status.clone(),
                timer_phase: db.timer.phase.clone(),
                daily_goal_minutes: db.settings.daily_goal_minutes,
                totals,
            },
        }
    }
}

struct Work {
    config: Config,
    config_error: Option<String>,
    status: ExportStatus,
    latest: Option<Arc<Statistics>>,
    pending: Option<Arc<Statistics>>,
    busy: bool,
    stopping: bool,
}
struct Shared {
    work: Mutex<Work>,
    changed: Condvar,
}

pub struct ExportService {
    shared: Arc<Shared>,
    // Configuration waits for an in-flight write, but enqueue never waits for vault I/O.
    io_gate: Arc<Mutex<()>>,
    config_path: PathBuf,
}

fn output_path(folder: &Path, os: &str) -> PathBuf {
    folder
        .join("Miku Focus")
        .join("Данные")
        .join(format!("{os}.json"))
}

fn config_error(message: impl std::fmt::Display) -> String {
    format!("Настройки автовыгрузки повреждены или недоступны: {message}. Оригинал не изменён.")
}

impl ExportService {
    pub fn new(root: &Path, os: &str, notify: impl Fn(ExportStatus) + Send + 'static) -> Self {
        let config_path = root.join("obsidian.json");
        let fresh = || Config {
            schema_version: SCHEMA,
            source_id: format!(
                "{os}-{}-{}-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ),
            folder: None,
        };
        let loaded = match fs::read(&config_path) {
            Ok(bytes) => serde_json::from_slice::<Config>(&bytes)
                .map_err(config_error)
                .and_then(|c| {
                    if c.schema_version != SCHEMA
                        || c.source_id.is_empty()
                        || c.folder.as_ref().is_some_and(|p| !p.is_absolute())
                    {
                        Err(config_error("неподдерживаемый формат"))
                    } else {
                        Ok(c)
                    }
                }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(fresh()),
            Err(e) => Err(config_error(e)),
        };
        let error = loaded.as_ref().err().cloned();
        let config = loaded.unwrap_or_else(|_| fresh());
        let status = ExportStatus {
            folder: config.folder.clone(),
            file: config.folder.as_ref().map(|p| output_path(p, os)),
            source_os: os.into(),
            source_id: config.source_id.clone(),
            exporting: false,
            last_exported_at: None,
            error: error.clone(),
        };
        let shared = Arc::new(Shared {
            work: Mutex::new(Work {
                config,
                config_error: error,
                status,
                latest: None,
                pending: None,
                busy: false,
                stopping: false,
            }),
            changed: Condvar::new(),
        });
        let io_gate = Arc::new(Mutex::new(()));
        let (thread_shared, gate) = (shared.clone(), io_gate.clone());
        std::thread::spawn(move || worker(thread_shared, gate, notify));
        Self {
            shared,
            io_gate,
            config_path,
        }
    }

    pub fn status(&self) -> ExportStatus {
        self.shared.work.lock().unwrap().status.clone()
    }

    /// Called only after explicit folder selection (or disconnect), outside the timer lock.
    pub fn configure(&self, folder: Option<&Path>) -> Result<(), String> {
        let _gate = self.io_gate.lock().unwrap();
        let (mut config, status) = {
            let work = self.shared.work.lock().unwrap();
            if let Some(error) = &work.config_error {
                return Err(error.clone());
            }
            (work.config.clone(), work.status.clone())
        };
        config.folder = folder
            .map(|folder| prepare_folder(folder, &status))
            .transpose()?;
        if self.config_path.exists() {
            fs::copy(
                &self.config_path,
                self.config_path.with_file_name("obsidian.backup.json"),
            )
            .map_err(|e| e.to_string())?;
        }
        store::atomic_write(
            &self.config_path,
            &serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?,
        )?;
        let mut work = self.shared.work.lock().unwrap();
        work.status.folder = config.folder.clone();
        work.status.file = config
            .folder
            .as_ref()
            .map(|p| output_path(p, &status.source_os));
        work.status.last_exported_at = None;
        work.status.error = None;
        work.status.exporting = false;
        work.config = config;
        work.latest = None;
        work.pending = None;
        self.shared.changed.notify_all();
        Ok(())
    }

    /// At most one pending snapshot. No disk access, timers, or unbounded channel.
    pub fn enqueue(&self, db: &Database, force: bool) {
        let mut work = self.shared.work.lock().unwrap();
        if work.config.folder.is_none() || work.config_error.is_some() {
            return;
        }
        let next = Statistics::capture(db);
        if !force
            && work.status.error.is_none()
            && work
                .latest
                .as_ref()
                .is_some_and(|old| old.content == next.content)
        {
            return;
        }
        let next = Arc::new(next);
        work.latest = Some(next.clone());
        work.pending = Some(next);
        work.status.exporting = true;
        self.shared.changed.notify_all();
    }

    /// Reuse the existing Rust service wake-up; successful idle exports do no more I/O.
    pub fn retry(&self) {
        let mut work = self.shared.work.lock().unwrap();
        if work.status.error.is_some()
            && work.config.folder.is_some()
            && !work.busy
            && work.pending.is_none()
        {
            work.pending = work.latest.clone();
            work.status.exporting = work.pending.is_some();
            self.shared.changed.notify_all();
        }
    }

    /// Exit may wait briefly for the last snapshot, never indefinitely for a missing disk.
    pub fn flush(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut work = self.shared.work.lock().unwrap();
        while work.busy || work.pending.is_some() {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            work = self.shared.changed.wait_timeout(work, remaining).unwrap().0;
        }
        true
    }
}

impl Drop for ExportService {
    fn drop(&mut self) {
        self.shared.work.lock().unwrap().stopping = true;
        self.shared.changed.notify_all();
    }
}

fn worker(shared: Arc<Shared>, gate: Arc<Mutex<()>>, notify: impl Fn(ExportStatus)) {
    loop {
        {
            let mut work = shared.work.lock().unwrap();
            while work.pending.is_none() && !work.stopping {
                work = shared.changed.wait(work).unwrap();
            }
            if work.stopping {
                return;
            }
        }
        let _gate = gate.lock().unwrap();
        let (snapshot, status) = {
            let mut work = shared.work.lock().unwrap();
            if work.stopping {
                return;
            }
            let Some(snapshot) = work.pending.take() else {
                continue;
            };
            work.busy = true;
            (snapshot, work.status.clone())
        };
        notify(status.clone());
        let result = write_snapshot(&status, &snapshot);
        let status = {
            let mut work = shared.work.lock().unwrap();
            work.busy = false;
            work.status.exporting = work.pending.is_some();
            match result {
                Ok(exported_at) => {
                    work.status.last_exported_at = Some(exported_at);
                    work.status.error = None;
                }
                Err(error) => {
                    work.status.error = Some(format!(
                        "Автовыгрузка не обновлена: {error}. История сохранена локально."
                    ));
                }
            }
            shared.changed.notify_all();
            work.status.clone()
        };
        notify(status);
    }
}

fn directory(path: &Path, create: bool) -> Result<(), String> {
    if create {
        match fs::create_dir(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    let meta = fs::symlink_metadata(path)
        .map_err(|e| format!("Папка {} недоступна: {e}", path.display()))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(format!(
            "{} должна быть обычной папкой, не ссылкой",
            path.display()
        ));
    }
    Ok(())
}

fn prepare_folder(folder: &Path, status: &ExportStatus) -> Result<PathBuf, String> {
    // Never create a missing vault/mount point. Only our two subdirectories are created.
    let folder =
        fs::canonicalize(folder).map_err(|e| format!("Не удалось открыть выбранную папку: {e}"))?;
    if folder.components().any(|part| {
        part.as_os_str()
            .to_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(".obsidian"))
    }) {
        return Err("Выберите корень хранилища, а не служебную папку .obsidian".into());
    }
    directory(&folder, false)?;
    let parent = folder.join("Miku Focus");
    directory(&parent, true)?;
    directory(&parent.join("Данные"), true)?;
    validate_existing(&output_path(&folder, &status.source_os), status)?;
    Ok(folder)
}

fn ordinary_file(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(format!(
            "{} — не обычный файл; он не изменён",
            path.display()
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn validate_existing(path: &Path, status: &ExportStatus) -> Result<(), String> {
    if !ordinary_file(path)? {
        return Ok(());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|_| {
            format!(
                "{} повреждён или не является выгрузкой; оригинал не изменён",
                path.display()
            )
        })?;
    if value["format"] != FORMAT
        || value["schemaVersion"] != SCHEMA
        || value["source"]["id"] != status.source_id
        || value["source"]["os"] != status.source_os
        || serde_json::from_value::<Statistics>(value).is_err()
    {
        return Err(format!(
            "{} принадлежит другому источнику или имеет неизвестный формат; оригинал не изменён",
            path.display()
        ));
    }
    Ok(())
}

fn write_snapshot(status: &ExportStatus, snapshot: &Statistics) -> Result<i64, String> {
    let folder = status.folder.as_ref().ok_or("Папка не подключена")?;
    directory(folder, false)?;
    directory(&folder.join("Miku Focus"), false)?;
    let dir = folder.join("Miku Focus").join("Данные");
    directory(&dir, false)?;
    let path = output_path(folder, &status.source_os);
    let lock_path = dir.join(format!("{}.lock", status.source_os));
    ordinary_file(&lock_path)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| e.to_string())?;
    lock.try_lock()
        .map_err(|_| "Файл выгрузки занят другим экземпляром Miku Focus".to_string())?;
    validate_existing(&path, status)?;
    let exported_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let mut document = serde_json::to_value(snapshot).map_err(|e| e.to_string())?;
    document["format"] = FORMAT.into();
    document["schemaVersion"] = SCHEMA.into();
    document["appVersion"] = env!("CARGO_PKG_VERSION").into();
    document["source"] = serde_json::json!({"id": status.source_id, "os": status.source_os});
    document["exportedAt"] = exported_at.into();
    atomic_export(
        &path,
        &serde_json::to_vec_pretty(&document).map_err(|e| e.to_string())?,
    )?;
    Ok(exported_at)
}

fn atomic_export(path: &Path, bytes: &[u8]) -> Result<(), String> {
    // create_new: even a stale or unrelated .tmp file must never be truncated.
    let temp = path.with_file_name(format!(
        ".{}.{}-{}.tmp",
        path.file_name().unwrap().to_string_lossy(),
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temp, path).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        std::fs::File::open(path.parent().unwrap())
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join(".local-test")
                .join(format!(
                    "obsidian-unit-{}-{}",
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_nanos(),
                    SEQUENCE.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn folder(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::create_dir(&path).unwrap();
            // The service stores canonical paths; Windows canonicalize also adds
            // a verbatim prefix. Compare equivalent canonical paths in tests.
            fs::canonicalize(path).unwrap()
        }
        fn service(&self, name: &str, os: &str) -> ExportService {
            ExportService::new(&self.folder(name), os, |_| {})
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            // Only this test's uniquely created directory; never personal app-data or vaults.
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn idle(service: &ExportService) {
        assert!(service.flush(Duration::from_secs(5)));
    }
    fn read(service: &ExportService) -> Value {
        idle(service);
        assert!(
            service.status().error.is_none(),
            "{:?}",
            service.status().error
        );
        serde_json::from_slice(&fs::read(service.status().file.unwrap()).unwrap()).unwrap()
    }
    fn focus(now: i64) -> Database {
        let mut db = Database::default();
        db.settings.focus_minutes = 1;
        db.action("start", 1000).unwrap();
        db.checkpoint(now)
    }

    #[test]
    fn materializes_current_work_at_capture_and_deadline_without_mutating_the_timer() {
        let db = focus(11000);
        let original = serde_json::to_value(&db).unwrap();
        let stats = Statistics::capture(&db);
        assert_eq!(stats.content.totals.work_ms, 10000);
        assert_eq!(stats.content.sessions.len(), 1);
        assert_eq!(
            stats.content.sessions[0].segments,
            vec![Segment {
                start_ms: 1000,
                end_ms: 11000
            }]
        );
        assert_eq!(
            stats.content.current_session_id,
            Some(stats.content.sessions[0].id.clone())
        );
        assert_eq!(
            Statistics::capture(&db.checkpoint(999999))
                .content
                .totals
                .work_ms,
            60000
        );
        assert_eq!(serde_json::to_value(&db).unwrap(), original);
    }

    #[test]
    fn pauses_decisions_extensions_and_rest_keep_the_existing_accounting() {
        let mut db = focus(11000);
        db.action("pause", 11000).unwrap();
        assert_eq!(
            Statistics::capture(&db.checkpoint(51000))
                .content
                .totals
                .work_ms,
            10000
        );
        db.action("resume", 51000).unwrap();
        db.tick(101000);
        assert_eq!(
            Statistics::capture(&db.checkpoint(501000))
                .content
                .totals
                .work_ms,
            60000
        );
        db.action("extend", 501000).unwrap();
        db.tick(801000);
        db.action("finish", 901000).unwrap();
        let stats = Statistics::capture(&db.checkpoint(911000));
        assert_eq!(stats.content.sessions.len(), 1);
        assert_eq!(stats.content.totals.work_ms, 360000);
        assert_eq!(stats.content.totals.break_ms, 10000);
        assert_eq!(stats.content.totals.completed_sessions, 1);
        assert_eq!(stats.content.totals.extensions, 1);
        assert_eq!(stats.content.breaks[0].start_ms, 901000);
        assert_eq!(
            Statistics::capture(&db.checkpoint(9999999))
                .content
                .totals
                .break_ms,
            300000
        );
    }

    #[test]
    fn windows_and_linux_exports_coexist_and_do_not_leak_private_settings_or_paths() {
        let f = Fixture::new();
        let vault = f.folder("Общее хранилище");
        fs::write(vault.join("Мои заметки.md"), "Не менять").unwrap();
        let linux = f.service("linux-data", "linux");
        let windows = f.service("windows-data", "windows");
        linux.configure(Some(&vault)).unwrap();
        windows.configure(Some(&vault)).unwrap();
        let mut db = focus(11000);
        db.settings.background = Some("/private/photo.png".into());
        db.settings
            .voices
            .insert("welcome".into(), "/private/voice.wav".into());
        linux.enqueue(&db, false);
        windows.enqueue(&focus(21000), false);
        let (a, b) = (read(&linux), read(&windows));
        assert_eq!(a["totals"]["workMs"], 10000);
        assert_eq!(b["totals"]["workMs"], 20000);
        assert_eq!(a["source"]["os"], "linux");
        assert_eq!(b["source"]["os"], "windows");
        assert_ne!(a["source"]["id"], b["source"]["id"]);
        assert_eq!(a["format"], FORMAT);
        assert_eq!(a["schemaVersion"], SCHEMA);
        assert!(a.get("settings").is_none());
        assert!(!a.to_string().contains("/private"));
        assert_eq!(
            fs::read_to_string(vault.join("Мои заметки.md")).unwrap(),
            "Не менять"
        );
    }

    #[test]
    fn unchanged_idle_snapshots_do_not_rewrite_and_disconnect_keeps_exported_files() {
        let f = Fixture::new();
        let vault = f.folder("vault");
        let service = f.service("data", "linux");
        service.configure(Some(&vault)).unwrap();
        let db = Database::default();
        service.enqueue(&db.checkpoint(1000), false);
        read(&service);
        let path = service.status().file.unwrap();
        let old = fs::read(&path).unwrap();
        service.enqueue(&db.checkpoint(9000), false);
        idle(&service);
        assert_eq!(fs::read(&path).unwrap(), old);
        service.enqueue(&db.checkpoint(10000), true);
        assert_eq!(read(&service)["capturedAt"], 10000);
        service.configure(None).unwrap();
        let backup: Value = serde_json::from_slice(
            &fs::read(service.config_path.with_file_name("obsidian.backup.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(backup["folder"], vault.to_string_lossy().as_ref());
        service.enqueue(&focus(21000), true);
        assert!(service.status().folder.is_none());
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap()["capturedAt"],
            10000
        );
    }

    #[test]
    fn connection_and_source_identity_survive_restart() {
        let f = Fixture::new();
        let data = f.folder("data");
        let vault = f.folder("vault");
        let original_id;
        {
            let service = ExportService::new(&data, "linux", |_| {});
            service.configure(Some(&vault)).unwrap();
            service.enqueue(&focus(11000), false);
            original_id = read(&service)["source"]["id"].clone();
        }
        let service = ExportService::new(&data, "linux", |_| {});
        assert_eq!(service.status().folder, Some(vault));
        service.enqueue(&focus(21000), false);
        let doc = read(&service);
        assert_eq!(doc["source"]["id"], original_id);
        assert_eq!(doc["totals"]["workMs"], 20000);
    }

    #[test]
    fn queue_is_bounded_latest_wins_and_timer_submission_does_not_wait_for_io() {
        let f = Fixture::new();
        let vault = f.folder("vault");
        let service = f.service("data", "linux");
        service.configure(Some(&vault)).unwrap();
        let gate = service.io_gate.lock().unwrap();
        for now in 1001..=1200 {
            service.enqueue(&focus(now), false);
        }
        assert_eq!(
            service
                .shared
                .work
                .lock()
                .unwrap()
                .pending
                .as_ref()
                .unwrap()
                .captured_at,
            1200
        );
        assert!(!service.flush(Duration::from_millis(10)));
        drop(gate);
        assert_eq!(read(&service)["totals"]["workMs"], 200);
    }

    #[test]
    fn missing_vault_is_not_recreated_and_retry_recovers_after_remount() {
        let f = Fixture::new();
        let vault = f.folder("vault");
        let service = f.service("data", "linux");
        service.configure(Some(&vault)).unwrap();
        service.enqueue(&focus(11000), false);
        read(&service);
        let offline = f.0.join("offline");
        fs::rename(&vault, &offline).unwrap();
        service.enqueue(&focus(21000), false);
        idle(&service);
        assert!(service
            .status()
            .error
            .unwrap()
            .contains("История сохранена локально"));
        assert!(!vault.exists());
        // An empty mount point is not treated as a new vault either.
        fs::create_dir(&vault).unwrap();
        service.retry();
        idle(&service);
        assert!(service.status().error.is_some());
        assert!(!vault.join("Miku Focus").exists());
        fs::remove_dir(&vault).unwrap();
        fs::rename(offline, &vault).unwrap();
        service.retry();
        assert_eq!(read(&service)["totals"]["workMs"], 20000);
    }

    #[test]
    fn malformed_export_and_other_installations_are_never_overwritten() {
        let f = Fixture::new();
        let vault = f.folder("vault");
        let service = f.service("data", "linux");
        service.configure(Some(&vault)).unwrap();
        service.enqueue(&focus(11000), false);
        read(&service);
        let other = f.service("other-data", "linux");
        assert!(other
            .configure(Some(&vault))
            .unwrap_err()
            .contains("другому источнику"));
        let path = service.status().file.unwrap();
        fs::write(&path, "не JSON").unwrap();
        service.enqueue(&focus(21000), false);
        idle(&service);
        assert!(service.status().error.is_some());
        assert_eq!(fs::read_to_string(path).unwrap(), "не JSON");
    }

    #[test]
    fn corrupt_config_does_not_block_the_app_or_destroy_the_original() {
        let f = Fixture::new();
        let data = f.folder("data");
        fs::write(data.join("obsidian.json"), "broken").unwrap();
        let service = ExportService::new(&data, "linux", |_| {});
        assert!(service.status().error.is_some());
        assert!(service.configure(Some(&f.folder("vault"))).is_err());
        assert!(service.configure(None).is_err());
        service.enqueue(&focus(11000), false);
        idle(&service);
        assert_eq!(
            fs::read_to_string(data.join("obsidian.json")).unwrap(),
            "broken"
        );
    }

    #[test]
    fn category_changes_are_exported_and_unrelated_temp_files_are_preserved() {
        let f = Fixture::new();
        let vault = f.folder("vault");
        let service = f.service("data", "linux");
        service.configure(Some(&vault)).unwrap();
        let mut db = Database::default().checkpoint(1000);
        service.enqueue(&db, false);
        read(&service);
        let stale = service.status().file.unwrap().with_extension("tmp");
        fs::write(&stale, "чужой файл").unwrap();
        db.categories[0].name = "Новая категория".into();
        service.enqueue(&db, false);
        assert_eq!(read(&service)["categories"][0]["name"], "Новая категория");
        assert_eq!(fs::read_to_string(stale).unwrap(), "чужой файл");
    }
    #[test]
    fn deleted_category_keeps_historical_labels_and_work_in_exports() {
        let f = Fixture::new();
        let vault = f.folder("vault");
        let service = f.service("data", "linux");
        service.configure(Some(&vault)).unwrap();
        let mut db = Database::default();
        db.action("start", 1000).unwrap();
        db.action("reset", 11000).unwrap();
        service.enqueue(&db.checkpoint(12000), false);
        let before = read(&service);
        db.delete_category("programming").unwrap();
        service.enqueue(&db.checkpoint(12000), false);
        let after = read(&service);
        assert_eq!(after["sessions"], before["sessions"]);
        let category = after["categories"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == "programming")
            .unwrap();
        assert_eq!(category["name"], "Программирование");
        assert_eq!(category["color"], "#55aebe");
        assert_eq!(category["deleted"], true);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_subdirectories_output_and_lock_are_rejected_without_touching_targets() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let vault = f.folder("vault");
        let other = f.folder("other");
        let service = f.service("data", "linux");
        symlink(&other, vault.join("Miku Focus")).unwrap();
        assert!(service.configure(Some(&vault)).is_err());
        assert_eq!(fs::read_dir(&other).unwrap().count(), 0);
        fs::remove_file(vault.join("Miku Focus")).unwrap();
        service.configure(Some(&vault)).unwrap();
        let path = service.status().file.unwrap();
        let target = other.join("note.md");
        fs::write(&target, "личные данные").unwrap();
        symlink(&target, &path).unwrap();
        service.enqueue(&focus(11000), false);
        idle(&service);
        assert!(service.status().error.is_some());
        fs::remove_file(&path).unwrap();
        fs::remove_file(path.with_extension("lock")).unwrap();
        symlink(&target, path.with_extension("lock")).unwrap();
        service.retry();
        idle(&service);
        assert!(service.status().error.is_some());
        assert_eq!(fs::read_to_string(target).unwrap(), "личные данные");
    }

    #[test]
    fn selecting_missing_folder_preserves_the_previous_configuration() {
        let f = Fixture::new();
        let vault = f.folder("vault");
        let service = f.service("data", "linux");
        service.configure(Some(&vault)).unwrap();
        let original = fs::read(&service.config_path).unwrap();
        assert!(service.configure(Some(&f.0.join("not-mounted"))).is_err());
        assert_eq!(fs::read(&service.config_path).unwrap(), original);
        assert_eq!(service.status().folder, Some(vault));
        assert_eq!(json!(service.status().exporting), false);
        let internal = f.folder(".obsidian");
        assert!(service
            .configure(Some(&internal))
            .unwrap_err()
            .contains("корень хранилища"));
        assert!(!internal.join("Miku Focus").exists());
    }
}
