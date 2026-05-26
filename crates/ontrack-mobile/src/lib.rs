// /qompassai/ontrack-rs/crates/ontrack-mobile/src/lib.rs
// Qompass AI — OnTrack mobile entry (Android + preview)
// Copyright (C) 2026 Qompass AI, All rights reserved.
// -----------------------------------------------------
//! OnTrack mobile crate.
//!
//! - `android_main` is the entry point for `cargo-apk` / android-activity.
//! - `desktop_main` runs the same Slint UI as a desktop preview binary.
//! - `controller` wires the Slint UI to `ontrack_core`.

pub mod controller;

#[cfg(target_os = "android")]
pub mod gps;

slint::include_modules!();

/// Bootstraps the Slint UI, attaches all callbacks, and runs the event loop.
pub fn run() -> anyhow::Result<()> {
    let ui = AppWindow::new()?;
    controller::wire(&ui)?;
    ui.run()?;
    Ok(())
}

/// Android entry point — called by `android-activity` via `cargo-apk`.
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    use android_logger::Config;
    use log::LevelFilter;

    android_logger::init_once(Config::default().with_max_level(LevelFilter::Info).with_tag("OnTrack"));
    log::info!("OnTrack Android starting");

    slint::android::init(app).expect("slint android init");
    if let Err(e) = run() {
        log::error!("OnTrack crashed: {e:?}");
    }
}
