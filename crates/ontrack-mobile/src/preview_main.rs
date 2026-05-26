// /qompassai/ontrack-rs/crates/ontrack-mobile/src/preview_main.rs
// Qompass AI — OnTrack mobile: desktop preview binary
// Copyright (C) 2026 Qompass AI, All rights reserved.
//
// Renders the Slint mobile UI in a desktop window for fast iteration.
fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).try_init().ok();
    ontrack_mobile::run()
}
