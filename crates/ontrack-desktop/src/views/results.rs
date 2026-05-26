
use eframe::egui;

use crate::app::OnTrackApp;

pub fn ui(app: &mut OnTrackApp, ui: &mut egui::Ui) {
    let (result, locations) = {
        let w = app.worker.lock().unwrap();
        (w.result.clone(), w.locations.clone())
    };
    let Some(result) = result else {
        ui.label("No route has been optimized yet. Go to the Home tab to add stops.");
        return;
    };

    ui.heading("Optimized Route");
    ui.label(format!(
        "Total drive time: {}   ·   Solver: {}",
        ontrack_core::exporter::format_duration(result.total_duration_seconds),
        result.backend_used
    ));
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        if ui.button("📋  Copy Google Maps URL").clicked() {
            let url = ontrack_core::exporter::build_maps_url(&result.ordered_addresses);
            ui.output_mut(|o| o.copied_text = url);
        }
        if ui.button("💾  Export CSV").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("CSV", &["csv"])
                .set_file_name("ontrack_route.csv")
                .save_file()
            {
                if let Err(e) = ontrack_core::exporter::export_csv(&result.ordered_addresses, &path)
                {
                    let mut w = app.worker.lock().unwrap();
                    w.error = Some(format!("export: {e}"));
                }
            }
        }
    });

    ui.add_space(8.0);
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (i, addr) in result.ordered_addresses.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{:>2}.", i + 1));
                ui.label(addr);
                let loc = locations
                    .iter()
                    .find(|l| l.address == *addr)
                    .cloned();
                if let Some(loc) = loc {
                    if let (Some(la), Some(ln)) = (loc.lat, loc.lng) {
                        if ui.button("Maps").clicked() {
                            let url = ontrack_core::exporter::build_maps_url(&vec![addr.clone()]);
                            ui.output_mut(|o| o.copied_text = url);
                        }
                        if ui.button("FieldMaps").clicked() {
                            let url = ontrack_core::exporter::build_fieldmaps_url(
                                addr,
                                Some(la),
                                Some(ln),
                                Some(&app.settings.arcgis_item_id),
                                2000,
                            );
                            ui.output_mut(|o| o.copied_text = url);
                        }
                        if ui.button("Waze").clicked() {
                            let url = ontrack_core::exporter::build_waze_url(la, ln);
                            ui.output_mut(|o| o.copied_text = url);
                        }
                    }
                }
            });
        }
    });
}
