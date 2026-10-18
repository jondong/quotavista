// Prevents an extra console window on Windows in release; the prototype is
// unsigned and macOS/Linux-only for now.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    quotavista_desktop_lib::run()
}
