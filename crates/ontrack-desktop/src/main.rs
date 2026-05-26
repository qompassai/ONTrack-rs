// /qompassai/ontrack-rs/crates/ontrack-desktop/src/main.rs
// Qompass AI — OnTrack desktop (egui/eframe)
// Copyright (C) 2026 Qompass AI, All rights reserved.
// -----------------------------------------------------
//! OnTrack desktop entry point.

mod app;
mod views;

use anyhow::Result;
use eframe::NativeOptions;

fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OnTrack — TDS Telecom Route Optimizer")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([720.0, 540.0]),
        ..Default::default()
    };

    eframe::run_native(
        "OnTrack",
        options,
        Box::new(|cc| Box::new(app::OnTrackApp::new(cc))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;
    Ok(())
}
