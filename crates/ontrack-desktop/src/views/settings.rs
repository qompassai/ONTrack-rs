
use std::fs;
use std::path::PathBuf;

use eframe::egui;

use crate::app::OnTrackApp;

pub fn ui(app: &mut OnTrackApp, ui: &mut egui::Ui) {
    ui.heading("Settings");
    ui.add_space(8.0);

    egui::Grid::new("settings_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Google Maps API key:");
        ui.add(egui::TextEdit::singleline(&mut app.settings.google_maps_api_key).desired_width(420.0).password(true));
        ui.end_row();

        ui.label("OSRM Base URL:");
        ui.add(egui::TextEdit::singleline(&mut app.settings.osrm_base_url).desired_width(420.0));
        ui.end_row();

        ui.label("ArcGIS Item ID:");
        ui.add(egui::TextEdit::singleline(&mut app.settings.arcgis_item_id).desired_width(420.0));
        ui.end_row();

        ui.label("Whisper model:");
        ui.add(egui::TextEdit::singleline(&mut app.settings.whisper_model).desired_width(160.0));
        ui.end_row();
    });

    ui.add_space(8.0);
    if ui.button("💾  Save to .env").clicked() {
        if let Err(e) = save_env(&app.settings) {
            let mut w = app.worker.lock().unwrap();
            w.error = Some(format!("save: {e}"));
        }
    }
    ui.add_space(8.0);
    ui.label("Note: keys are stored only in your local .env file — never transmitted to TDS servers.");
}

fn save_env(s: &ontrack_core::config::Settings) -> anyhow::Result<()> {
    let path = PathBuf::from(".env");
    let contents = format!(
        "GOOGLE_MAPS_API_KEY=\"{}\"\nOSRM_BASE_URL=\"{}\"\nARCGIS_ITEM_ID=\"{}\"\nONTRACK_WHISPER_MODEL=\"{}\"\n",
        s.google_maps_api_key, s.osrm_base_url, s.arcgis_item_id, s.whisper_model,
    );
    fs::write(path, contents)?;
    Ok(())
}
