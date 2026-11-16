//! IdleDetectionAdapter. No inference about productivity; never pauses a timer.
use tauri::{AppHandle, Manager};

fn report(app: &AppHandle, idle: bool) {
    let state = app.state::<crate::AppState>();
    let mut r = state.runtime.lock().unwrap();
    let was_idle = r.system_idle;
    r.system_idle = idle;
    r.idle_adapter = "available".into();
    if idle != was_idle {
        r.initiative(if idle { "idle" } else { "return" }, state.now());
    }
    drop(r);
    crate::publish(app);
}

#[cfg(target_os = "linux")]
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        if let Err(error) = wayland::run(app.clone()) {
            let state = app.state::<crate::AppState>();
            state.runtime.lock().unwrap().idle_adapter = format!("unavailable: {error}");
            crate::publish(&app);
        }
    });
}

#[cfg(target_os = "linux")]
mod wayland {
    use super::*;
    use wayland_client::{
        delegate_noop,
        protocol::{wl_registry, wl_seat},
        Connection, Dispatch, QueueHandle,
    };
    use wayland_protocols::ext::idle_notify::v1::client::{
        ext_idle_notification_v1, ext_idle_notifier_v1,
    };
    struct State {
        app: AppHandle,
        notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
        seat: Option<wl_seat::WlSeat>,
    }
    impl Dispatch<wl_registry::WlRegistry, ()> for State {
        fn event(
            s: &mut Self,
            registry: &wl_registry::WlRegistry,
            event: wl_registry::Event,
            _: &(),
            _: &Connection,
            q: &QueueHandle<Self>,
        ) {
            if let wl_registry::Event::Global {
                name,
                interface,
                version,
            } = event
            {
                if interface == "ext_idle_notifier_v1" {
                    s.notifier = Some(registry.bind(name, version.min(1), q, ()));
                }
                if interface == "wl_seat" && s.seat.is_none() {
                    s.seat = Some(registry.bind(name, version.min(1), q, ()));
                }
            }
        }
    }
    delegate_noop!(State: ignore wl_seat::WlSeat);
    delegate_noop!(State: ignore ext_idle_notifier_v1::ExtIdleNotifierV1);
    impl Dispatch<ext_idle_notification_v1::ExtIdleNotificationV1, ()> for State {
        fn event(
            s: &mut Self,
            _: &ext_idle_notification_v1::ExtIdleNotificationV1,
            event: ext_idle_notification_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                ext_idle_notification_v1::Event::Idled => report(&s.app, true),
                ext_idle_notification_v1::Event::Resumed => report(&s.app, false),
                _ => {}
            }
        }
    }
    pub fn run(app: AppHandle) -> Result<(), String> {
        let conn = Connection::connect_to_env().map_err(|e| e.to_string())?;
        let mut queue = conn.new_event_queue();
        let q = queue.handle();
        let _registry = conn.display().get_registry(&q, ());
        let mut state = State {
            app: app.clone(),
            notifier: None,
            seat: None,
        };
        queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
        let notifier = state
            .notifier
            .as_ref()
            .ok_or("ext-idle-notify-v1 not offered")?;
        let seat = state.seat.as_ref().ok_or("Wayland seat unavailable")?;
        let _notification = notifier.get_idle_notification(300_000, seat, &q, ());
        report(&app, false);
        loop {
            queue
                .blocking_dispatch(&mut state)
                .map_err(|e| e.to_string())?;
        }
    }
}

#[cfg(windows)]
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        use windows_sys::Win32::{
            System::SystemInformation::GetTickCount,
            UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
        };
        loop {
            let mut info = LASTINPUTINFO {
                cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
                dwTime: 0,
            };
            unsafe {
                if GetLastInputInfo(&mut info) != 0 {
                    report(&app, GetTickCount().wrapping_sub(info.dwTime) >= 300_000);
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(15));
        }
    });
}
#[cfg(not(any(target_os = "linux", windows)))]
pub fn start(_: AppHandle) {}
