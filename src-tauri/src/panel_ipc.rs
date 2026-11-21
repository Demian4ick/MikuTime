//! Linux-only, same-UID local IPC. One persistent Waybar client, no disk polling.
use crate::panel::{Hub, WaybarFrame};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::Shutdown,
    os::{
        fd::AsRawFd,
        linux::net::SocketAddrExt,
        unix::{
            ffi::OsStrExt,
            net::{SocketAddr, UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

const APP_ID: &str = "dev.mikufocus.desktop";
const MAX_CLIENTS: usize = 9; // eight streams plus a control request
const MAX_REQUEST: u64 = 256;

fn address(root: &Path) -> std::io::Result<SocketAddr> {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.into());
    let hash = root
        .as_os_str()
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    // SAFETY: getuid has no arguments or preconditions.
    let uid = unsafe { libc::getuid() };
    SocketAddr::from_abstract_name(format!("miku-focus-{uid}-{hash:016x}"))
}

fn same_user(stream: &UnixStream) -> bool {
    let mut credential = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: a live socket FD, correctly sized writable ucred and socklen_t.
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credential as *mut libc::ucred).cast(),
            &mut size,
        )
    };
    result == 0
        && size as usize == std::mem::size_of::<libc::ucred>()
        && credential.uid == unsafe { libc::getuid() }
}

fn root_from_environment() -> Result<PathBuf, String> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .ok_or("Не удалось определить папку данных Miku Focus")?;
    Ok(base.join(APP_ID))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    command: String,
}
#[derive(Serialize, Deserialize)]
struct Reply {
    ok: bool,
    error: Option<String>,
}

fn line(reader: impl Read, limit: u64) -> std::io::Result<String> {
    let mut line = String::new();
    BufReader::new(reader.take(limit + 1)).read_line(&mut line)?;
    if line.len() as u64 > limit || !line.ends_with('\n') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Неверное сообщение панели",
        ));
    }
    Ok(line)
}

fn reply(stream: &mut UnixStream, result: Result<(), String>) -> std::io::Result<()> {
    let reply = Reply {
        ok: result.is_ok(),
        error: result.err(),
    };
    writeln!(stream, "{}", serde_json::to_string(&reply)?)
}

fn connection(
    mut stream: UnixStream,
    hub: Arc<Hub>,
    show: Arc<dyn Fn() -> Result<(), String> + Send + Sync>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    if !same_user(&stream) {
        return;
    }
    let request = line(&mut stream, MAX_REQUEST)
        .ok()
        .and_then(|text| serde_json::from_str::<Request>(&text).ok());
    match request.as_ref().map(|r| r.command.as_str()) {
        Some("show") => {
            let _ = reply(&mut stream, show());
        }
        Some("watch") => {
            let Some(slot) = hub.subscribe() else {
                return;
            };
            // A blocking reader detects EOF immediately, even when an idle timer sends no events.
            // Shutdown always wakes it; join bounds the number of reader threads as well.
            let Ok(mut reader) = stream.try_clone() else {
                return;
            };
            let _ = reader.set_read_timeout(None);
            let closed = slot.clone();
            let reader = std::thread::spawn(move || {
                let _ = reader.read(&mut [0u8; 1]);
                closed.close();
                let _ = reader.shutdown(Shutdown::Both);
            });
            while let Some(view) = slot.next() {
                let Ok(text) = serde_json::to_string(&view.waybar()) else {
                    break;
                };
                if writeln!(stream, "{text}").is_err() {
                    break;
                }
            }
            slot.close();
            let _ = stream.shutdown(Shutdown::Both);
            let _ = reader.join();
        }
        _ => {
            let _ = reply(&mut stream, Err("Неизвестная команда панели".into()));
        }
    }
}

pub struct Server {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    pub fn start(
        root: &Path,
        hub: Arc<Hub>,
        show: Arc<dyn Fn() -> Result<(), String> + Send + Sync>,
    ) -> Result<Self, String> {
        let address = address(root).map_err(|e| e.to_string())?;
        let listener = UnixListener::bind_addr(&address).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let clients = Arc::new(AtomicUsize::new(0));
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stopping.load(Ordering::Acquire) {
                    break;
                }
                let Ok(stream) = stream else {
                    break;
                };
                if clients.load(Ordering::Acquire) >= MAX_CLIENTS {
                    continue;
                }
                clients.fetch_add(1, Ordering::AcqRel);
                let (hub, show, clients) = (hub.clone(), show.clone(), clients.clone());
                std::thread::spawn(move || {
                    connection(stream, hub, show);
                    clients.fetch_sub(1, Ordering::AcqRel);
                });
            }
        });
        Ok(Self {
            address,
            stop,
            thread: Some(thread),
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = UnixStream::connect_addr(&self.address); // wake blocking accept; no polling timer
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn connect(root: &Path, command: &str) -> Result<UnixStream, String> {
    let mut stream = UnixStream::connect_addr(&address(root).map_err(|e| e.to_string())?)
        .map_err(|_| "Miku Focus не запущен или подключение панели недоступно".to_string())?;
    if !same_user(&stream) {
        return Err("Панель принадлежит другому пользователю".into());
    }
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    writeln!(stream, "{}", serde_json::json!({"command": command})).map_err(|e| e.to_string())?;
    Ok(stream)
}

fn watch(root: &Path) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let Ok(stream) = connect(root, "watch") else {
        let _ = writeln!(
            out,
            "{}",
            serde_json::json!({"text":"", "tooltip":"Miku Focus не запущен", "class":"offline", "alt":"offline"})
        );
        return Ok(());
    };
    let mut reader = BufReader::new(stream);
    loop {
        let mut text = String::new();
        let length = reader
            .by_ref()
            .take(8193)
            .read_line(&mut text)
            .map_err(|e| e.to_string())?;
        if length == 0 {
            break;
        }
        if length > 8192
            || !text.ends_with('\n')
            || serde_json::from_str::<WaybarFrame>(&text).is_err()
        {
            return Err("Неверный снимок панели".into());
        }
        if out
            .write_all(text.as_bytes())
            .and_then(|_| out.flush())
            .is_err()
        {
            return Ok(());
        }
    }
    // Clear the last displayed time when the app closes; Waybar can reconnect on restart.
    let _ = writeln!(
        out,
        "{}",
        serde_json::json!({"text":"", "tooltip":"Miku Focus закрыт", "class":"offline", "alt":"offline"})
    );
    Ok(())
}

pub fn run_cli() -> Option<i32> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !args.iter().any(|a| a == "--waybar" || a == "--show-main") {
        return None;
    }
    let result = (|| {
        if args.len() != 1 {
            return Err("Используйте --waybar или --show-main без других аргументов".into());
        }
        let root = root_from_environment()?;
        if args[0] == "--waybar" {
            return watch(&root);
        }
        let mut stream = connect(&root, "show")?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        let response: Reply =
            serde_json::from_str(&line(&mut stream, 8192).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if response.ok {
            Ok(())
        } else {
            Err(response.error.unwrap_or("Не удалось открыть окно".into()))
        }
    })();
    Some(match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        panel::View,
        timer::{BackgroundMode, Database},
    };
    fn unique_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "miku-panel-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn identifier_matches_tauri_and_abstract_address_supports_long_app_data_paths() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["identifier"], APP_ID);
        let root = unique_root().join("очень-длинный-путь-к-тестовым-данным".repeat(12));
        let hub = Arc::new(Hub::new());
        let _server = Server::start(&root, hub.clone(), Arc::new(|| Ok(()))).unwrap();
        let mut stream = connect(&root, "watch").unwrap();
        let first: WaybarFrame = serde_json::from_str(&line(&mut stream, 8192).unwrap()).unwrap();
        assert_eq!(first.class, "disabled");
        let mut db = Database::default();
        db.settings.background_mode = BackgroundMode::Panel;
        hub.publish(View::capture(&db, 0));
        let next: WaybarFrame = serde_json::from_str(&line(&mut stream, 8192).unwrap()).unwrap();
        assert_eq!(next.text, "25:00");
        stream.shutdown(Shutdown::Both).unwrap();
        hub.close();
    }
    #[test]
    fn commands_are_bounded_and_only_show_can_invoke_the_callback() {
        let root = unique_root();
        let hub = Arc::new(Hub::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let _server = Server::start(
            &root,
            hub.clone(),
            Arc::new(move || {
                observed.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        )
        .unwrap();
        for (command, expected) in [("erase-history", false), ("show", true)] {
            let mut stream = connect(&root, command).unwrap();
            let response: Reply = serde_json::from_str(&line(&mut stream, 8192).unwrap()).unwrap();
            assert_eq!(response.ok, expected);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(line(&b"not-terminated"[..], MAX_REQUEST).is_err());
        assert!(line(&vec![b'x'; 1024][..], MAX_REQUEST).is_err());
        hub.close();
    }
}
