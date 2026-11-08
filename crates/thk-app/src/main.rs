#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(unsafe_code)]

#[cfg(not(target_os = "android"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    thk_app::run_desktop_app()
}

#[cfg(target_os = "android")]
fn main() {}
