//! PlatformWindowAdapter. No shell strings, no user configuration writes.
use serde_json::Value;
use tauri::{AppHandle, Manager};

pub fn is_hyprland() -> bool {
    cfg!(target_os = "linux") && std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
}

/// Called before GTK starts threads. Only changes this process, never user config.
pub fn configure_graphics() {
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none()
        && std::env::var("GBM_BACKEND").as_deref() == Ok("nvidia-drm")
        && !std::path::Path::new("/usr/lib/gbm/nvidia-drm_gbm.so").exists()
    {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        eprintln!("Miku Focus: missing NVIDIA GBM backend; using app-local WebKit compatibility renderer. System settings unchanged.");
    }
}

#[cfg(unix)]
fn hypr(command: &str) -> Result<String, String> {
    use std::{
        io::{Read, Write},
        os::unix::net::UnixStream,
        time::Duration,
    };
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or("No XDG_RUNTIME_DIR")?;
    let signature =
        std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").ok_or("No Hyprland instance")?;
    let path = std::path::PathBuf::from(runtime)
        .join("hypr")
        .join(signature)
        .join(".socket.sock");
    let mut socket = UnixStream::connect(path).map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    socket
        .write_all(command.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut reply = String::new();
    socket
        .read_to_string(&mut reply)
        .map_err(|e| e.to_string())?;
    Ok(reply)
}
#[cfg(not(unix))]
fn hypr(_: &str) -> Result<String, String> {
    Err("Hyprland is unavailable".into())
}
fn clients() -> Result<Value, String> {
    serde_json::from_str(&hypr("j/clients")?).map_err(|e| e.to_string())
}
pub fn own_client(title: &str) -> Result<Value, String> {
    clients()?
        .as_array()
        .ok_or("Invalid compositor reply")?
        .iter()
        .find(|c| {
            c["pid"].as_u64() == Some(std::process::id() as u64)
                && c["title"].as_str() == Some(title)
        })
        .cloned()
        .ok_or("Окно ещё не отображено compositor".into())
}
#[derive(Clone, Copy)]
enum WindowAction {
    Float,
    Resize(u32, u32),
    Move(i32, i32),
    Raise,
}
impl WindowAction {
    fn command(self, address: &str, pid: u32, lua: bool) -> Result<String, String> {
        let digits = address.strip_prefix("0x").ok_or("Invalid window address")?;
        if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("Invalid window address".into());
        }
        if lua {
            let action = match self {
                Self::Float => "float({window=w,action=\"enable\"})".into(),
                Self::Resize(x, y) => format!("resize({{window=w,x={x},y={y},relative=false}})"),
                Self::Move(x, y) => format!("move({{window=w,x={x},y={y},relative=false}})"),
                Self::Raise => "alter_zorder({window=w,mode=\"top\"})".into(),
            };
            // Resolve once and check ownership inside the compositor, before dispatch.
            Ok(format!("dispatch function() local w=hl.get_window(\"address:{address}\"); assert(w and w.pid=={pid},\"Owned window unavailable\"); hl.dispatch(hl.dsp.window.{action}) end"))
        } else {
            let (name, argument) = match self {
                Self::Float => ("setfloating", format!("address:{address}")),
                Self::Resize(x, y) => (
                    "resizewindowpixel",
                    format!("exact {x} {y},address:{address}"),
                ),
                Self::Move(x, y) => (
                    "movewindowpixel",
                    format!("exact {x} {y},address:{address}"),
                ),
                Self::Raise => ("alterzorder", format!("top,address:{address}")),
            };
            Ok(format!("dispatch {name} {argument}"))
        }
    }
}
fn dispatch(address: &str, action: WindowAction) -> Result<(), String> {
    static LUA: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let lua =
        *LUA.get_or_init(|| hypr("repl return type(hl.dsp)").is_ok_and(|s| s.trim() == "table"));
    let pid = std::process::id();
    let command = action.command(address, pid, lua)?;
    if !lua
        && !clients()?.as_array().is_some_and(|items| {
            items.iter().any(|c| {
                c["address"].as_str() == Some(address) && c["pid"].as_u64() == Some(u64::from(pid))
            })
        })
    {
        return Err("Собственное окно недоступно".into());
    }
    let text = hypr(&command)?;
    if !text.trim().starts_with("ok") {
        return Err(format!("Hyprland: {text}"));
    }
    Ok(())
}
pub fn prepare(
    app: &AppHandle,
    label: &str,
    top: bool,
    pos: Option<(i32, i32)>,
    size: (u32, u32),
) -> Result<(), String> {
    let window = app.get_webview_window(label).ok_or("Окно недоступно")?;
    if !is_hyprland() {
        if label == "companion" {
            window.set_resizable(true).map_err(|e| e.to_string())?;
            window
                .set_size(tauri::LogicalSize::new(
                    f64::from(size.0),
                    f64::from(size.1),
                ))
                .map_err(|e| e.to_string())?;
            window.set_always_on_top(top).map_err(|e| e.to_string())?;
            if let Some((x, y)) = pos {
                window
                    .set_position(tauri::PhysicalPosition::new(x, y))
                    .map_err(|e| e.to_string())?;
            }
        }
        return Ok(());
    }
    let title = if label == "companion" {
        "Miku Focus — помощник"
    } else {
        "Miku Focus"
    };
    let client = own_client(title)?;
    let address = client["address"].as_str().ok_or("No window address")?;
    // This only changes our live window, never a rule or configuration file.
    dispatch(address, WindowAction::Float)?;
    let (w, h) = if label == "companion" {
        (size.0 as i32, size.1 as i32)
    } else {
        (760, 570)
    };
    dispatch(address, WindowAction::Resize(w as u32, h as u32))?;
    let monitors: Value = serde_json::from_str(&hypr("j/monitors")?).map_err(|e| e.to_string())?;
    if let Some(monitor) = monitors
        .as_array()
        .and_then(|ms| ms.iter().find(|m| m["id"] == client["monitor"]))
    {
        let scale = monitor["scale"].as_f64().unwrap_or(1.0).max(0.1);
        let mx = monitor["x"].as_i64().unwrap_or(0) as i32;
        let my = monitor["y"].as_i64().unwrap_or(0) as i32;
        let mw = (monitor["width"].as_f64().unwrap_or(1920.0) / scale) as i32;
        let mh = (monitor["height"].as_f64().unwrap_or(1080.0) / scale) as i32;
        let default = if label == "companion" {
            (mx + mw - w - 30, my + mh - h - 30)
        } else {
            (mx + (mw - w) / 2, my + (mh - h) / 2)
        };
        let (x, y) = if label == "companion" {
            pos.unwrap_or(default)
        } else {
            default
        };
        let (x, y) = (
            x.clamp(mx, (mx + mw - w).max(mx)),
            y.clamp(my + 32, (my + mh - h).max(my + 32)),
        );
        dispatch(address, WindowAction::Move(x, y))?;
    }
    if label == "companion" {
        if top {
            raise(address)?;
        }
    }
    Ok(())
}
pub fn raise(address: &str) -> Result<(), String> {
    dispatch(address, WindowAction::Raise)
}

pub struct DragOrigin {
    address: String,
    origin: (i32, i32),
    cursor: (i32, i32),
    pub last: (i32, i32),
}
fn cursor() -> Result<(i32, i32), String> {
    let p: Value = serde_json::from_str(&hypr("j/cursorpos")?).map_err(|e| e.to_string())?;
    Ok((
        p["x"].as_i64().ok_or("No cursor x")? as i32,
        p["y"].as_i64().ok_or("No cursor y")? as i32,
    ))
}
pub fn begin_drag() -> Result<DragOrigin, String> {
    let c = own_client("Miku Focus — помощник")?;
    let origin = (
        c["at"][0].as_i64().ok_or("No x")? as i32,
        c["at"][1].as_i64().ok_or("No y")? as i32,
    );
    Ok(DragOrigin {
        address: c["address"].as_str().ok_or("No address")?.into(),
        origin,
        cursor: cursor()?,
        last: origin,
    })
}
pub fn move_drag(drag: &mut DragOrigin) -> Result<(), String> {
    let p = cursor()?;
    drag.last = (
        drag.origin.0 + p.0 - drag.cursor.0,
        drag.origin.1 + p.1 - drag.cursor.1,
    );
    dispatch(&drag.address, WindowAction::Move(drag.last.0, drag.last.1))
}
pub fn position(app: &AppHandle) -> Option<(i32, i32)> {
    if is_hyprland() {
        let c = own_client("Miku Focus — помощник").ok()?;
        Some((c["at"][0].as_i64()? as i32, c["at"][1].as_i64()? as i32))
    } else {
        let p = app.get_webview_window("companion")?.outer_position().ok()?;
        Some((p.x, p.y))
    }
}

pub fn size(app: &AppHandle) -> Option<(u32, u32)> {
    if is_hyprland() {
        let c = own_client("Miku Focus — помощник").ok()?;
        Some((c["size"][0].as_u64()? as u32, c["size"][1].as_u64()? as u32))
    } else {
        let window = app.get_webview_window("companion")?;
        let size = window
            .inner_size()
            .ok()?
            .to_logical::<u32>(window.scale_factor().ok()?);
        Some((size.width, size.height))
    }
}

pub fn resize_to(app: &AppHandle, size: (u32, u32)) -> Result<(), String> {
    if is_hyprland() {
        let c = own_client("Miku Focus — помощник")?;
        let address = c["address"].as_str().ok_or("Нет адреса окна")?;
        let position = (
            c["at"][0].as_i64().ok_or("Нет координаты окна")? as i32,
            c["at"][1].as_i64().ok_or("Нет координаты окна")? as i32,
        );
        resize_anchored(address, size, position)
    } else {
        app.get_webview_window("companion")
            .ok_or("Окно недоступно")?
            .set_size(tauri::LogicalSize::new(
                f64::from(size.0),
                f64::from(size.1),
            ))
            .map_err(|e| e.to_string())
    }
}

pub struct ResizeOrigin {
    address: String,
    origin: (u32, u32),
    cursor: (i32, i32),
    position: (i32, i32),
    pub last: (u32, u32),
}

pub fn begin_resize() -> Result<ResizeOrigin, String> {
    let c = own_client("Miku Focus — помощник")?;
    let origin = (
        c["size"][0].as_u64().ok_or("Нет ширины окна")? as u32,
        c["size"][1].as_u64().ok_or("Нет высоты окна")? as u32,
    );
    Ok(ResizeOrigin {
        address: c["address"].as_str().ok_or("Нет адреса окна")?.into(),
        origin,
        cursor: cursor()?,
        position: (
            c["at"][0].as_i64().ok_or("Нет координаты окна")? as i32,
            c["at"][1].as_i64().ok_or("Нет координаты окна")? as i32,
        ),
        last: origin,
    })
}

pub fn move_resize(resize: &mut ResizeOrigin) -> Result<(), String> {
    let p = cursor()?;
    resize.last = crate::window_geometry::resized_size(
        resize.origin,
        (
            p.0.saturating_sub(resize.cursor.0),
            p.1.saturating_sub(resize.cursor.1),
        ),
    );
    resize_anchored(&resize.address, resize.last, resize.position)
}

fn resize_anchored(address: &str, size: (u32, u32), position: (i32, i32)) -> Result<(), String> {
    dispatch(address, WindowAction::Resize(size.0, size.1))?;
    // Hyprland's dispatcher resizes around the center; the south-east grip must keep the top-left fixed.
    dispatch(address, WindowAction::Move(position.0, position.1))
}

/// Event-driven raise on compositor focus changes, not a polling subprocess.
#[cfg(unix)]
pub fn watch_focus(app: AppHandle) {
    use std::{
        io::{BufRead, BufReader},
        os::unix::net::UnixStream,
    };
    if !is_hyprland() {
        return;
    }
    std::thread::spawn(move || {
        let Some(signature) = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE") else {
            return;
        };
        let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") else {
            return;
        };
        let socket = std::path::PathBuf::from(runtime)
            .join("hypr")
            .join(signature)
            .join(".socket2.sock");
        let Ok(stream) = UnixStream::connect(socket) else {
            return;
        };
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            if !line.starts_with("activewindowv2>>") {
                continue;
            }
            let state = app.state::<crate::AppState>();
            let should_raise = {
                let r = state.runtime.lock().unwrap();
                r.active_window == "companion" && r.db.settings.always_on_top
            };
            if should_raise {
                if let Ok(c) = own_client("Miku Focus — помощник") {
                    if let Some(address) = c["address"].as_str() {
                        let _ = raise(address);
                    }
                }
            }
        }
    });
}
#[cfg(not(unix))]
pub fn watch_focus(_: AppHandle) {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dispatchers_are_addressed_and_lua_checks_the_owning_process() {
        for action in [
            WindowAction::Float,
            WindowAction::Resize(180, 190),
            WindowAction::Move(-12, 34),
            WindowAction::Raise,
        ] {
            let lua = action.command("0xabCD", 123, true).unwrap();
            assert!(lua.contains("hl.get_window(\"address:0xabCD\")"));
            assert!(lua.contains("w.pid==123"));
            assert!(lua.contains("window=w"));
            let legacy = action.command("0xabCD", 123, false).unwrap();
            assert!(legacy.ends_with("address:0xabCD"));
        }
        assert!(WindowAction::Move(-12, 34)
            .command("0x1", 1, true)
            .unwrap()
            .contains("x=-12,y=34,relative=false"));
        assert_eq!(
            WindowAction::Resize(180, 190)
                .command("0x1", 1, false)
                .unwrap(),
            "dispatch resizewindowpixel exact 180 190,address:0x1"
        );
    }
    #[test]
    fn malformed_compositor_addresses_cannot_become_lua_or_legacy_commands() {
        for address in ["", "0x", "activewindow", "0x1\n", "0x1\"); error(1)--"] {
            for lua in [true, false] {
                assert!(WindowAction::Raise.command(address, 1, lua).is_err());
            }
        }
    }
}
