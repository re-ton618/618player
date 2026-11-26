//! Runs the player on the machine's native backend (`cargo run`).

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
use nsh_platform_linux as platform;
#[cfg(target_os = "macos")]
use nsh_platform_macos as platform;
#[cfg(target_os = "windows")]
use nsh_platform_windows as platform;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn main() -> impl std::process::Termination {
    env_logger::init();
    platform::run_app(
        platform::AppSource::Rust(platform::Root::of::<rust_music_poc::App>()),
        platform::AppOptions {
            title: rust_music_poc::TITLE.to_owned(),
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            app_id: rust_music_poc::APP_ID.to_owned(),
            size: rust_music_poc::DEFAULT_SIZE,
            ..Default::default()
        },
    )
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!("Run this binary on macOS, Windows or Linux; use `nsh run` for other targets.");
