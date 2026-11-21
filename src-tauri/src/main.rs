#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    if let Some(code) = miku_focus_lib::run_panel_cli() {
        std::process::exit(code);
    }
    miku_focus_lib::run();
}
