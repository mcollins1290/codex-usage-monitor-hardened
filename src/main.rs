#![windows_subsystem = "windows"]

mod app_server;
mod codex_colors;
mod diagnose;
mod localization;
mod models;
mod native_interop;
mod poller;
mod privacy;
mod provider_icons;
mod provider_poll;
mod quota_alerts;
mod quota_refresh;
mod quota_text;
mod quota_tooltip;
mod recovery_events;
mod reset_alerts;
mod settings_store;
mod theme;
mod tray_icon;
mod window;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    privacy::initialize(&args);
    let diagnose_enabled = args.iter().any(|arg| arg == "--diagnose");
    if diagnose_enabled {
        match diagnose::init() {
            Ok(path) => {
                diagnose::log(format!("startup args={args:?} log_path={}", path.display()));
                diagnose::log(format!("version={}", env!("CARGO_PKG_VERSION")));
            }
            Err(error) => {
                // Logging may not be available yet, but keep startup behavior unchanged.
                let _ = error;
            }
        }
    }

    if diagnose_enabled {
        diagnose::log("entering window::run");
    }
    // No window takes text input. Without this, focusing Settings loads the
    // user's input method (often with Direct2D and ~10 threads) for good.
    unsafe {
        let _ = windows::Win32::UI::Input::Ime::ImmDisableIME(u32::MAX);
    }
    window::run();
}
