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
fn dispatch(name: &str, argument: &str) -> Result<(), String> {
    let text = hypr(&format!("dispatch {name} {argument}"))?;
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
    dispatch("setfloating", &format!("address:{address}"))?;
    let (w, h) = if label == "companion" {
        (size.0 as i32, size.1 as i32)
    } else {
        (760, 570)
    };
    dispatch(
        "resizewindowpixel",
        &format!("exact {w} {h},address:{address}"),
    )?;
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
        dispatch(
            "movewindowpixel",
            &format!("exact {x} {y},address:{address}"),
        )?;
    }
    if label == "companion" {
        if top {
            raise(address)?;
        }
    }
    Ok(())
}
pub fn raise(address: &str) -> Result<(), String> {
    dispatch("alterzorder", &format!("top,address:{address}"))
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
    dispatch(
        "movewindowpixel",
        &format!(
            "exact {} {},address:{}",
            drag.last.0, drag.last.1, drag.address
        ),
    )
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
    dispatch(
        "resizewindowpixel",
        &format!("exact {} {},address:{address}", size.0, size.1),
    )?;
    // Hyprland's dispatcher resizes around the center; the south-east grip must keep the top-left fixed.
    dispatch(
        "movewindowpixel",
        &format!("exact {} {},address:{address}", position.0, position.1),
    )
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
