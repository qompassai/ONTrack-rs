// /qompassai/ontrack-rs/crates/ontrack-desktop/src/views/home.rs
// Qompass AI — OnTrack desktop: Home view (address entry + import)
// Copyright (C) 2026 Qompass AI, All rights reserved.
// --------------------------------------------------------------------
//! Address entry, CSV/Excel import, current-location seed, and the
//! "Optimize Route" trigger.

use eframe::egui;
use ontrack_core::matrix::Backend;

use crate::app::OnTrackApp;

pub fn ui(app: &mut OnTrackApp, ui: &mut egui::Ui) {
    ui.heading("Stops");
    ui.add_space(4.0);

    // Add controls
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut app.address_input)
                .hint_text("Type an address and press Enter…")
                .desired_width(420.0),
        );
        let add_clicked = ui.button("Add").clicked();
        if (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || add_clicked {
            let s = app.address_input.trim().to_string();
            if !s.is_empty() {
                app.addresses.push(s);
                app.address_input.clear();
            }
        }

        if ui.button("Import CSV/Excel").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("CSV / Excel", &["csv", "xlsx", "xls", "xlsm", "ods"])
                .pick_file()
            {
                match ontrack_core::parser::parse_addresses(&path) {
                    Ok(mut addrs) => app.addresses.append(&mut addrs),
                    Err(e) => {
                        let mut w = app.worker.lock().unwrap();
                        w.error = Some(format!("parse: {e}"));
                    }
                }
            }
        }

        if ui.button("Use Current Location").clicked() {
            if let Some(loc) = ontrack_core::geocoder::get_current_location() {
                app.addresses.insert(0, loc.address);
            }
        }
    });

    ui.add_space(8.0);

    // Address list
    let mut to_remove: Option<usize> = None;
    egui::ScrollArea::vertical()
        .max_height(360.0)
        .show(ui, |ui| {
            for (i, addr) in app.addresses.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{:>2}.", i + 1));
                    ui.label(addr);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✕").clicked() {
                            to_remove = Some(i);
                        }
                    });
                });
            }
        });
    if let Some(i) = to_remove {
        app.addresses.remove(i);
    }

    ui.add_space(12.0);
    ui.separator();

    // Backend selection
    ui.horizontal(|ui| {
        ui.label("Distance backend:");
        egui::ComboBox::from_id_source("backend_combo")
            .selected_text(format!("{:?}", app.backend))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.backend, Backend::Osrm, "OSRM (free)");
                ui.selectable_value(&mut app.backend, Backend::Google, "Google");
                ui.selectable_value(&mut app.backend, Backend::Haversine, "Haversine");
            });
        ui.checkbox(&mut app.use_google, "Use Google for geocoding");
    });

    ui.add_space(8.0);

    let (busy, status, progress, err) = {
        let w = app.worker.lock().unwrap();
        (
            w.busy,
            w.status_line.clone(),
            w.progress,
            w.error.clone(),
        )
    };

    ui.horizontal(|ui| {
        let can_run = !busy && !app.addresses.is_empty();
        if ui
            .add_enabled(can_run, egui::Button::new("🚀  Optimize Route"))
            .clicked()
        {
            app.run_optimize();
        }
        if busy {
            ui.spinner();
            ui.label(&status);
            if progress.1 > 0 {
                ui.label(format!("{}/{}", progress.0, progress.1));
            }
        }
    });

    if let Some(e) = err {
        ui.add_space(8.0);
        ui.colored_label(egui::Color32::LIGHT_RED, format!("Error: {e}"));
    }
}
