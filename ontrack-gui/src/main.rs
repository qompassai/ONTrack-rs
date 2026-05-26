//! ontrack-gui — TDS Telecom Field Route Optimizer (egui desktop app)
//!
//! Three-panel layout matching the Python CustomTkinter version:
//!   Left panel  — stop list (add/remove/reorder)
//!   Center      — route results table + action buttons
//!   Right panel — location/map preview + per-stop navigation actions
//!
//! All network calls (geocoding, matrix, tile fetch) run on a Tokio thread
//! pool and send results back to the UI via crossbeam channels.
//!
//! Copyright (C) 2025 Qompass AI, All rights reserved

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use crossbeam_channel::{bounded, Receiver, Sender};
use eframe::egui::{self, Color32, FontId, RichText, ScrollArea, TextEdit, Ui};
use rfd::FileDialog;
use ontrack_core::{
    config::Config,
    exporter::{build_fieldmaps_url, build_maps_url, build_maps_url_chunked,
               build_osm_tile_url, build_waze_url, export_csv},
    geocoder::{geocode_addresses, get_current_location_ip, Location},
    matrix::{build_distance_matrix, MatrixBackend},
    parser::parse_addresses,
    solver::{solve_tsp, RouteResult, SolverBackend},
};
use std::str::FromStr;
use std::sync::Arc;
use tokio::runtime::Runtime;

// ── Brand colours (TDS palette) ────────────────────────────────────────────
const TDS_BLUE:   Color32 = Color32::from_rgb(0x00, 0x57, 0xA8);
const TDS_NAVY:   Color32 = Color32::from_rgb(0x00, 0x28, 0x55);
const TDS_ORANGE: Color32 = Color32::from_rgb(0xF2, 0x65, 0x22);
const TDS_BG:     Color32 = Color32::from_rgb(0x11, 0x18, 0x27);
const TDS_SURFACE:Color32 = Color32::from_rgb(0x1A, 0x25, 0x35);
const TDS_CARD:   Color32 = Color32::from_rgb(0x24, 0x30, 0x44);
const TDS_GRAY:   Color32 = Color32::from_rgb(0x6B, 0x72, 0x80);
const TDS_WHITE:  Color32 = Color32::WHITE;
const TDS_GREEN:  Color32 = Color32::from_rgb(0x22, 0xC5, 0x5E);
const TDS_RED:    Color32 = Color32::from_rgb(0xEF, 0x44, 0x44);

// ── Background task messages ───────────────────────────────────────────────

enum WorkerMsg {
    Progress(String),
    Done { result: RouteResult, locations: Vec<Location> },
    Error(String),
    LocationFound(Location),
    TileLoaded { idx: usize, bytes: Vec<u8> },
}

// ── App state ──────────────────────────────────────────────────────────────

#[derive(Default, PartialEq, Eq)]
enum AppPanel {
    #[default]
    NewRoute,
    Results,
    Settings,
}

struct OnTrackApp {
    // Configuration
    config: Config,

    // ── New Route panel ──
    address_input:   String,
    stop_list:       Vec<String>,
    route_type:      String,  // "open" | "round trip"
    backend:         String,  // "osrm" | "google" | "haversine"
    solver_backend:  String,  // "2opt" | "nn"
    show_advanced:   bool,

    // ── Results panel ──
    route_result:    Option<RouteResult>,
    locations:       Vec<Location>,
    add_stop_input:  String,
    selected_stop:   Option<usize>,
    preview_texture: Option<egui::TextureHandle>,
    preview_source:  String,

    // ── Settings panel ──
    s_google_key:    String,
    s_osrm_url:      String,
    s_arcgis_id:     String,
    settings_status: String,

    // ── Navigation ──
    current_panel:   AppPanel,

    // ── Background worker ──
    status_msg:      String,
    is_working:      bool,
    tx:              Sender<WorkerMsg>,
    rx:              Receiver<WorkerMsg>,
    rt:              Arc<Runtime>,
}

impl OnTrackApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Dark theme
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = TDS_BG;
        cc.egui_ctx.set_style(style);

        let config = Config::from_env();
        let (tx, rx) = bounded::<WorkerMsg>(64);
        let rt = Arc::new(Runtime::new().expect("Tokio runtime"));

        let s_google_key = config.google_maps_api_key.clone().unwrap_or_default();
        let s_osrm_url = config.osrm_base_url.clone();
        let s_arcgis_id = config.arcgis_item_id.clone().unwrap_or_default();

        Self {
            config,
            address_input: String::new(),
            stop_list: Vec::new(),
            route_type: "open".to_string(),
            backend: "osrm".to_string(),
            solver_backend: "2opt".to_string(),
            show_advanced: false,
            route_result: None,
            locations: Vec::new(),
            add_stop_input: String::new(),
            selected_stop: None,
            preview_texture: None,
            preview_source: "OpenStreetMap".to_string(),
            s_google_key,
            s_osrm_url,
            s_arcgis_id,
            settings_status: String::new(),
            current_panel: AppPanel::NewRoute,
            status_msg: "Add stops to get started.".to_string(),
            is_working: false,
            tx,
            rx,
            rt,
        }
    }

    // ── Top navigation bar ─────────────────────────────────────────────────

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar")
            .frame(egui::Frame::default().fill(TDS_NAVY).inner_margin(8.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("🗺  OnTrack")
                            .color(TDS_WHITE)
                            .font(FontId::proportional(20.0))
                            .strong(),
                    );
                    ui.label(
                        RichText::new("  TDS Telecom · Field Route Optimizer")
                            .color(TDS_GRAY)
                            .font(FontId::proportional(12.0)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for (panel, label) in [
                            (AppPanel::Settings, "Settings"),
                            (AppPanel::Results,  "Route"),
                            (AppPanel::NewRoute, "New Route"),
                        ] {
                            let active = self.current_panel == panel;
                            let btn = egui::Button::new(
                                RichText::new(label)
                                    .color(TDS_WHITE)
                                    .strong()
                            )
                            .fill(if active { TDS_BLUE } else { Color32::TRANSPARENT });
                            if ui.add(btn).clicked() {
                                self.current_panel = panel;
                            }
                        }
                    });
                });
            });
    }

    // ── New Route panel ────────────────────────────────────────────────────

    fn panel_new_route(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("stop_list_panel")
            .resizable(true)
            .default_width(320.0)
            .frame(egui::Frame::default().fill(TDS_SURFACE).inner_margin(12.0))
            .show(ctx, |ui| {
                ui.label(RichText::new("Stop List").color(TDS_WHITE).strong().size(16.0));
                ui.add_space(6.0);

                // Address entry
                ui.horizontal(|ui| {
                    let resp = ui.add(
                        TextEdit::singleline(&mut self.address_input)
                            .hint_text("Enter address…")
                            .desired_width(ui.available_width() - 60.0)
                    );
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.add_address_from_input();
                    }
                    if ui.button("Add").clicked() {
                        self.add_address_from_input();
                    }
                });

                ui.add_space(4.0);

                // Action row
                ui.horizontal(|ui| {
                    if ui.button("📂 Load File").clicked() {
                        self.load_file_dialog();
                    }
                    if ui.button("📍 My Location").clicked() {
                        self.fetch_current_location();
                    }
                    let clr = egui::Button::new(RichText::new("🗑 Clear").color(TDS_WHITE))
                        .fill(Color32::from_rgb(0x3B, 0x1A, 0x1A));
                    if ui.add(clr).clicked() {
                        self.stop_list.clear();
                    }
                });

                ui.add_space(4.0);

                // Reorder row
                ui.horizontal(|ui| {
                    if ui.small_button("▲ Up").clicked() { self.move_stop(-1); }
                    if ui.small_button("▼ Down").clicked() { self.move_stop(1); }
                    let del = egui::Button::new(RichText::new("✕ Remove").color(TDS_WHITE))
                        .fill(Color32::from_rgb(0x3B, 0x1A, 0x1A));
                    if ui.add_sized([80.0, 22.0], del).clicked() {
                        if let Some(idx) = self.selected_stop {
                            if idx < self.stop_list.len() {
                                self.stop_list.remove(idx);
                                self.selected_stop = None;
                            }
                        }
                    }
                });

                ui.add_space(4.0);

                // Stop list (scrollable, selectable rows)
                ScrollArea::vertical().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for (i, addr) in self.stop_list.iter().enumerate() {
                        let selected = self.selected_stop == Some(i);
                        let row = egui::SelectableLabel::new(
                            selected,
                            RichText::new(format!("{:>2}. {addr}", i + 1))
                                .color(if selected { TDS_ORANGE } else { TDS_WHITE })
                                .size(13.0),
                        );
                        if ui.add(row).clicked() {
                            self.selected_stop = Some(i);
                        }
                    }
                });

                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("{} stops", self.stop_list.len()))
                        .color(TDS_GRAY)
                        .size(11.0),
                );
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(TDS_SURFACE).inner_margin(20.0))
            .show(ctx, |ui| {
                ui.label(RichText::new("Route Options").color(TDS_WHITE).strong().size(16.0));
                ui.add_space(8.0);

                // Simple options (always visible)
                egui::Grid::new("opts_grid")
                    .num_columns(2)
                    .spacing([16.0, 10.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("Route type:").color(TDS_GRAY));
                        egui::ComboBox::from_id_source("route_type")
                            .selected_text(&self.route_type)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.route_type, "open".into(), "open (no return)");
                                ui.selectable_value(&mut self.route_type, "round trip".into(), "round trip");
                            });
                        ui.end_row();
                    });

                ui.add_space(4.0);

                // Advanced toggle
                ui.horizontal(|ui| {
                    let arrow = if self.show_advanced { "▼" } else { "▶" };
                    if ui.add(egui::Button::new(
                        RichText::new(format!("{arrow} Advanced options")).color(TDS_GRAY).size(12.0)
                    ).fill(Color32::TRANSPARENT)).clicked() {
                        self.show_advanced = !self.show_advanced;
                    }
                });

                if self.show_advanced {
                    egui::Frame::default()
                        .fill(TDS_CARD)
                        .rounding(8.0)
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            egui::Grid::new("adv_grid")
                                .num_columns(2)
                                .spacing([16.0, 8.0])
                                .show(ui, |ui| {
                                    ui.label(RichText::new("Distance backend:").color(TDS_GRAY).size(12.0));
                                    egui::ComboBox::from_id_source("backend")
                                        .selected_text(&self.backend)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut self.backend, "osrm".into(), "osrm (free)");
                                            ui.selectable_value(&mut self.backend, "google".into(), "google (API key)");
                                            ui.selectable_value(&mut self.backend, "haversine".into(), "haversine (straight-line)");
                                        });
                                    ui.end_row();

                                    ui.label(RichText::new("Solver:").color(TDS_GRAY).size(12.0));
                                    egui::ComboBox::from_id_source("solver")
                                        .selected_text(&self.solver_backend)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut self.solver_backend, "2opt".into(), "2-opt (better quality)");
                                            ui.selectable_value(&mut self.solver_backend, "nn".into(), "nearest-neighbor (faster)");
                                        });
                                    ui.end_row();
                                });
                        });
                }

                ui.add_space(8.0);

                // Status
                ui.label(RichText::new(&self.status_msg).color(TDS_GRAY).size(12.0));
                ui.add_space(4.0);

                if self.is_working {
                    ui.spinner();
                }

                ui.add_space(8.0);

                // Solve button
                let btn = egui::Button::new(
                    RichText::new("⚡  Optimize Route")
                        .color(TDS_WHITE)
                        .strong()
                        .size(16.0),
                )
                .fill(TDS_ORANGE)
                .min_size(egui::vec2(ui.available_width(), 52.0));

                if !self.is_working && ui.add(btn).clicked() {
                    self.start_solve();
                }

                ui.add_space(12.0);
                ui.label(
                    RichText::new(
                        "Tip: Click '▲▼' to reorder stops before solving.\n\
                         All route/map features work without any API key."
                    )
                    .color(TDS_GRAY)
                    .size(11.0),
                );
            });
    }

    // ── Results panel ──────────────────────────────────────────────────────

    fn panel_results(&mut self, ctx: &egui::Context) {
        let result = match &self.route_result {
            Some(r) => r.clone(),
            None => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new("No route yet — go to New Route to start.")
                            .color(TDS_GRAY).size(16.0));
                    });
                });
                return;
            }
        };

        // Summary banner
        egui::TopBottomPanel::top("summary_bar")
            .frame(egui::Frame::default().fill(TDS_NAVY).inner_margin(8.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "✓  {} stops  ·  {}  ·  solver: {:?}",
                            result.ordered_addresses.len(),
                            result.format_duration(),
                            result.backend_used,
                        ))
                        .color(TDS_WHITE)
                        .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new("💾 Export CSV").color(TDS_WHITE)).fill(TDS_CARD)).clicked() {
                            self.export_csv_dialog();
                        }
                        if ui.add(egui::Button::new(RichText::new("↩ Waze").color(TDS_WHITE)).fill(TDS_CARD)).clicked() {
                            if let Some(l) = self.locations.first() {
                                if let (Some(lat), Some(lng)) = (l.lat, l.lng) {
                                    let _ = open::that(build_waze_url(lat, lng));
                                }
                            }
                        }
                        if ui.add(egui::Button::new(RichText::new("📐 FieldMaps").color(TDS_WHITE)).fill(Color32::from_rgb(0x1A, 0x5F, 0x3F))).clicked() {
                            self.open_fieldmaps(0);
                        }
                        if ui.add(egui::Button::new(RichText::new("🗺 Open in Maps").color(TDS_WHITE)).fill(TDS_BLUE)).clicked() {
                            self.open_maps_all();
                        }
                    });
                });
            });

        // Right panel — preview
        egui::SidePanel::right("preview_panel")
            .resizable(true)
            .default_width(320.0)
            .frame(egui::Frame::default().fill(TDS_SURFACE).inner_margin(12.0))
            .show(ctx, |ui| {
                ui.label(RichText::new("Location Preview").color(TDS_WHITE).strong().size(15.0));

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(
                            self.selected_stop
                                .and_then(|i| result.ordered_addresses.get(i))
                                .map(|a| format!("Stop {}: {}", self.selected_stop.unwrap() + 1, a))
                                .unwrap_or_else(|| "Select a stop".into()),
                        )
                        .color(TDS_GRAY)
                        .size(11.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(&self.preview_source).color(TDS_GRAY).size(10.0));
                    });
                });

                // Map preview image
                let preview_size = egui::vec2(290.0, 190.0);
                if let Some(tex) = &self.preview_texture {
                    egui::Frame::default().rounding(8.0).show(ui, |ui| {
                        ui.image((tex.id(), preview_size));
                    });
                } else {
                    egui::Frame::default()
                        .fill(TDS_CARD)
                        .rounding(8.0)
                        .show(ui, |ui| {
                            ui.set_min_size(preview_size);
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    RichText::new("Select a stop\nto see its location")
                                        .color(TDS_GRAY),
                                );
                            });
                        });
                }

                ui.add_space(8.0);

                // Per-stop action buttons
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("🗺 Navigate").color(TDS_WHITE)).fill(TDS_BLUE)).clicked() {
                        if let Some(i) = self.selected_stop {
                            self.open_maps_single(i);
                        }
                    }
                    if ui.add(egui::Button::new(RichText::new("📐 FieldMaps").color(TDS_WHITE)).fill(Color32::from_rgb(0x1A, 0x5F, 0x3F))).clicked() {
                        if let Some(i) = self.selected_stop {
                            self.open_fieldmaps(i);
                        }
                    }
                    if ui.add(egui::Button::new(RichText::new("🌐 Street View").color(TDS_WHITE)).fill(TDS_CARD)).clicked() {
                        if let Some(i) = self.selected_stop {
                            self.open_streetview(i);
                        }
                    }
                });

                ui.add_space(8.0);

                // Re-optimize button
                let re_btn = egui::Button::new(
                    RichText::new("⚡ Re-optimize Route").color(TDS_WHITE).strong()
                )
                .fill(TDS_ORANGE)
                .min_size(egui::vec2(ui.available_width(), 40.0));
                if ui.add(re_btn).clicked() {
                    // Push current addresses back to stop list and switch to New Route
                    self.stop_list = result.ordered_addresses.clone();
                    self.current_panel = AppPanel::NewRoute;
                }
            });

        // Centre — stop table
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(TDS_SURFACE).inner_margin(12.0))
            .show(ctx, |ui| {
                // Inline add stop
                ui.horizontal(|ui| {
                    let resp = ui.add(
                        TextEdit::singleline(&mut self.add_stop_input)
                            .hint_text("Add a stop to the route…")
                            .desired_width(ui.available_width() - 70.0),
                    );
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.add_inline_stop();
                    }
                    if ui.button("+ Add").clicked() {
                        self.add_inline_stop();
                    }
                });

                ui.add_space(6.0);

                // Route table header
                ui.horizontal(|ui| {
                    ui.label(RichText::new("  #").color(TDS_GRAY).strong().size(12.0));
                    ui.add_space(24.0);
                    ui.label(RichText::new("Address").color(TDS_GRAY).strong().size(12.0));
                });
                ui.separator();

                ScrollArea::vertical().show(ui, |ui| {
                    let addrs = result.ordered_addresses.clone();
                    let mut to_delete: Option<usize> = None;
                    let mut to_move: Option<(usize, i32)> = None;

                    for (i, addr) in addrs.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let selected = self.selected_stop == Some(i);
                            let num_lbl = RichText::new(format!("{:>2}.", i + 1))
                                .color(TDS_ORANGE)
                                .strong()
                                .size(13.0);

                            if ui.selectable_label(selected, num_lbl).clicked() {
                                self.selected_stop = Some(i);
                                self.load_preview_for_stop(i);
                            }

                            let addr_lbl = RichText::new(addr)
                                .color(if selected { TDS_ORANGE } else { TDS_WHITE })
                                .size(12.0);
                            if ui
                                .add(egui::SelectableLabel::new(selected, addr_lbl))
                                .clicked()
                            {
                                self.selected_stop = Some(i);
                                self.load_preview_for_stop(i);
                            }

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let del = egui::Button::new(RichText::new("✕").color(TDS_WHITE))
                                    .fill(Color32::from_rgb(0x3B, 0x1A, 0x1A));
                                if ui.add(del).clicked() { to_delete = Some(i); }
                                if ui.small_button("▼").clicked() { to_move = Some((i, 1)); }
                                if ui.small_button("▲").clicked() { to_move = Some((i, -1)); }
                            });
                        });
                        ui.separator();
                    }

                    // Apply mutations after the loop
                    if let Some(idx) = to_delete {
                        if let Some(r) = &mut self.route_result {
                            r.ordered_addresses.remove(idx);
                            r.ordered_indices.remove(idx);
                        }
                        if self.locations.len() > idx {
                            self.locations.remove(idx);
                        }
                    }
                    if let Some((idx, dir)) = to_move {
                        let new_idx = (idx as i64 + dir as i64) as usize;
                        if let Some(r) = &mut self.route_result {
                            if new_idx < r.ordered_addresses.len() {
                                r.ordered_addresses.swap(idx, new_idx);
                                r.ordered_indices.swap(idx, new_idx);
                                if self.locations.len() == r.ordered_addresses.len() {
                                    self.locations.swap(idx, new_idx);
                                }
                            }
                        }
                    }
                });
            });
    }

    // ── Settings panel ─────────────────────────────────────────────────────

    fn panel_settings(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(TDS_BG).inner_margin(20.0))
            .show(ctx, |ui| {
                ui.label(RichText::new("API Configuration").color(TDS_WHITE).strong().size(16.0));
                ui.add_space(12.0);

                egui::Frame::default().fill(TDS_CARD).rounding(8.0).inner_margin(14.0).show(ui, |ui| {
                    self.settings_field(ui, "Google Maps API Key",
                        "Required for Street View, Google geocoding, and Google distance matrix.\n\
                         Without it, Nominatim + OSRM are used (free, no setup).",
                        &mut self.s_google_key.clone(), true);
                });
                ui.add_space(8.0);

                egui::Frame::default().fill(TDS_CARD).rounding(8.0).inner_margin(14.0).show(ui, |ui| {
                    self.settings_field(ui, "OSRM Base URL",
                        "Custom OSRM server. Default: http://router.project-osrm.org",
                        &mut self.s_osrm_url.clone(), false);
                });
                ui.add_space(8.0);

                egui::Frame::default().fill(TDS_CARD).rounding(8.0).inner_margin(14.0).show(ui, |ui| {
                    self.settings_field(ui, "ArcGIS Web Map Item ID",
                        "Your ArcGIS Online Web Map ID for FieldMaps deep links.",
                        &mut self.s_arcgis_id.clone(), false);
                });
                ui.add_space(16.0);

                let save_btn = egui::Button::new(RichText::new("💾 Save Settings").color(TDS_WHITE).strong())
                    .fill(TDS_BLUE)
                    .min_size(egui::vec2(200.0, 40.0));
                if ui.add(save_btn).clicked() {
                    self.save_settings();
                }

                if !self.settings_status.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(&self.settings_status).color(TDS_GREEN).size(12.0));
                }

                ui.add_space(20.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(RichText::new("About OnTrack").color(TDS_WHITE).strong().size(14.0));
                ui.label(
                    RichText::new(
                        "OnTrack v2.0  ·  TDS Telecom Internal\n\
                         Rust implementation — core + CLI + egui GUI\n\n\
                         All features work without an API key:\n\
                         • Routing: OSRM (free)\n\
                         • Geocoding: Nominatim / OpenStreetMap (free)\n\
                         • Map preview: OSM tiles (free)\n\
                         • Navigation: Google Maps URL scheme (no key)\n\
                         • FieldMaps: deep link (uses your existing app)\n\n\
                         Optional with Google API key:\n\
                         • Street View photo previews\n\
                         • Google geocoding (better rural accuracy)\n\
                         • Google distance matrix (live traffic)"
                    )
                    .color(TDS_GRAY)
                    .size(12.0),
                );
            });
    }

    fn settings_field(&mut self, ui: &mut Ui, label: &str, hint: &str, value: &mut String, _secret: bool) {
        ui.label(RichText::new(label).color(TDS_WHITE).strong().size(13.0));
        ui.label(RichText::new(hint).color(TDS_GRAY).size(11.0));
        ui.add_space(4.0);
        // Re-bind to real field based on label
        match label {
            "Google Maps API Key" => { ui.add(TextEdit::singleline(&mut self.s_google_key).password(true).desired_width(400.0)); }
            "OSRM Base URL" => { ui.add(TextEdit::singleline(&mut self.s_osrm_url).desired_width(400.0)); }
            "ArcGIS Web Map Item ID" => { ui.add(TextEdit::singleline(&mut self.s_arcgis_id).desired_width(400.0)); }
            _ => {}
        }
    }

    // ── Action helpers ─────────────────────────────────────────────────────

    fn add_address_from_input(&mut self) {
        let addr = self.address_input.trim().to_string();
        if !addr.is_empty() {
            self.stop_list.push(addr);
            self.address_input.clear();
        }
    }

    fn add_inline_stop(&mut self) {
        let addr = self.add_stop_input.trim().to_string();
        if !addr.is_empty() {
            if let Some(r) = &mut self.route_result {
                r.ordered_addresses.push(addr);
            }
            self.add_stop_input.clear();
        }
    }

    fn move_stop(&mut self, dir: i64) {
        if let Some(idx) = self.selected_stop {
            let new_idx = idx as i64 + dir;
            if new_idx >= 0 && (new_idx as usize) < self.stop_list.len() {
                self.stop_list.swap(idx, new_idx as usize);
                self.selected_stop = Some(new_idx as usize);
            }
        }
    }

    fn load_file_dialog(&mut self) {
        if let Some(path) = FileDialog::new()
            .add_filter("Address files", &["csv", "tsv", "txt"])
            .pick_file()
        {
            match parse_addresses(&path) {
                Ok(addrs) => {
                    self.stop_list.extend(addrs);
                    self.status_msg = format!("Loaded {} addresses.", self.stop_list.len());
                }
                Err(e) => self.status_msg = format!("Load error: {e}"),
            }
        }
    }

    fn fetch_current_location(&mut self) {
        let tx = self.tx.clone();
        let http = reqwest::Client::builder()
            .user_agent("OnTrack-TDS/2.0")
            .build()
            .unwrap();
        self.rt.spawn(async move {
            if let Some(loc) = get_current_location_ip(&http).await {
                let _ = tx.send(WorkerMsg::LocationFound(loc));
            }
        });
        self.status_msg = "Detecting location…".into();
    }

    fn start_solve(&mut self) {
        if self.stop_list.len() < 2 {
            self.status_msg = "Add at least 2 stops.".into();
            return;
        }

        self.is_working = true;
        self.status_msg = "Geocoding…".into();

        let addresses = self.stop_list.clone();
        let backend_str = self.backend.clone();
        let solver_str = self.solver_backend.clone();
        let open_route = self.route_type == "open";
        let config = self.config.clone();
        let tx = self.tx.clone();

        self.rt.spawn(async move {
            let http = reqwest::Client::builder()
                .user_agent("OnTrack-TDS/2.0")
                .build()
                .unwrap();

            let tx2 = tx.clone();
            let locations = geocode_addresses(
                &http,
                &addresses,
                config.has_google_key() && backend_str == "google",
                config.google_maps_api_key.as_deref(),
                move |done, total| {
                    let _ = tx2.send(WorkerMsg::Progress(format!("Geocoding {done}/{total}…")));
                },
            )
            .await;

            let _ = tx.send(WorkerMsg::Progress("Building distance matrix…".into()));
            let matrix_backend = MatrixBackend::from_str(&backend_str)
                .unwrap_or_default();

            let (resolved, matrix) = match build_distance_matrix(
                &http,
                &locations,
                &matrix_backend,
                &config.osrm_base_url,
                config.google_maps_api_key.as_deref(),
            )
            .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(WorkerMsg::Error(e.to_string()));
                    return;
                }
            };

            let _ = tx.send(WorkerMsg::Progress("Solving route…".into()));
            let solver_backend = SolverBackend::from_str(&solver_str).unwrap_or_default();

            match solve_tsp(&resolved, &matrix, 0, &solver_backend, open_route) {
                Ok(result) => {
                    let _ = tx.send(WorkerMsg::Done { result, locations: resolved });
                }
                Err(e) => {
                    let _ = tx.send(WorkerMsg::Error(e.to_string()));
                }
            }
        });
    }

    fn load_preview_for_stop(&mut self, idx: usize) {
        let result = match &self.route_result { Some(r) => r, None => return };
        let addr = match result.ordered_addresses.get(idx) { Some(a) => a.clone(), None => return };
        let lat_lng = self.locations.iter().find(|l| l.address == addr)
            .and_then(|l| if l.is_resolved() { Some((l.lat.unwrap(), l.lng.unwrap())) } else { None });

        if let Some((lat, lng)) = lat_lng {
            self.preview_source = "OpenStreetMap".into();
            let url = build_osm_tile_url(lat, lng, 16);
            let tx = self.tx.clone();
            let http = reqwest::Client::builder().user_agent("OnTrack-TDS/2.0").build().unwrap();
            self.rt.spawn(async move {
                if let Ok(resp) = http.get(&url).send().await {
                    if let Ok(bytes) = resp.bytes().await {
                        let _ = tx.send(WorkerMsg::TileLoaded { idx, bytes: bytes.to_vec() });
                    }
                }
            });
        }
    }

    fn open_maps_all(&self) {
        if let Some(r) = &self.route_result {
            let urls = if r.ordered_addresses.len() <= 10 {
                vec![build_maps_url(&r.ordered_addresses)]
            } else {
                build_maps_url_chunked(&r.ordered_addresses)
            };
            for url in urls { let _ = open::that(url); }
        }
    }

    fn open_maps_single(&self, idx: usize) {
        if let Some(r) = &self.route_result {
            if let Some(addr) = r.ordered_addresses.get(idx) {
                let url = format!(
                    "https://www.google.com/maps/dir/?api=1&destination={}&travelmode=driving",
                    urlencoding::encode(addr)
                );
                let _ = open::that(url);
            }
        }
    }

    fn open_fieldmaps(&self, idx: usize) {
        if let Some(r) = &self.route_result {
            if let Some(addr) = r.ordered_addresses.get(idx) {
                let loc = self.locations.iter().find(|l| &l.address == addr);
                let url = build_fieldmaps_url(
                    addr,
                    loc.and_then(|l| l.lat),
                    loc.and_then(|l| l.lng),
                    self.config.arcgis_item_id.as_deref(),
                    2000,
                );
                let _ = open::that(url);
            }
        }
    }

    fn open_streetview(&self, idx: usize) {
        if let Some(r) = &self.route_result {
            if let Some(addr) = r.ordered_addresses.get(idx) {
                let loc = self.locations.iter().find(|l| &l.address == addr);
                let url = if let Some(l) = loc {
                    if let (Some(lat), Some(lng)) = (l.lat, l.lng) {
                        format!("https://www.google.com/maps/@{lat},{lng},3a,90y,0h,90t/data=!3m4!1e1")
                    } else {
                        format!("https://www.google.com/maps/search/{}", urlencoding::encode(addr))
                    }
                } else {
                    format!("https://www.google.com/maps/search/{}", urlencoding::encode(addr))
                };
                let _ = open::that(url);
            }
        }
    }

    fn export_csv_dialog(&self) {
        if let Some(r) = &self.route_result {
            if let Some(path) = FileDialog::new()
                .set_file_name("ontrack_route.csv")
                .add_filter("CSV", &["csv"])
                .save_file()
            {
                let _ = export_csv(&r.ordered_addresses, path);
            }
        }
    }

    fn save_settings(&mut self) {
        // Update config in memory
        self.config.google_maps_api_key = if self.s_google_key.is_empty() { None } else { Some(self.s_google_key.clone()) };
        self.config.osrm_base_url = self.s_osrm_url.clone();
        self.config.arcgis_item_id = if self.s_arcgis_id.is_empty() { None } else { Some(self.s_arcgis_id.clone()) };

        // Persist to .env
        let env_path = ".env";
        let content = format!(
            "GOOGLE_MAPS_API_KEY=\"{}\"\nOSRM_BASE_URL=\"{}\"\nARCGIS_ITEM_ID=\"{}\"\n",
            self.s_google_key, self.s_osrm_url, self.s_arcgis_id
        );
        if std::fs::write(env_path, content).is_ok() {
            self.settings_status = "✓ Settings saved to .env".into();
        } else {
            self.settings_status = "⚠ Could not write .env".into();
        }
    }

    // ── Poll background worker channel ─────────────────────────────────────

    fn poll_worker(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                WorkerMsg::Progress(s) => {
                    self.status_msg = s;
                    ctx.request_repaint();
                }
                WorkerMsg::Done { result, locations } => {
                    self.status_msg = format!(
                        "✓ Route ready: {} stops · {}",
                        result.ordered_addresses.len(),
                        result.format_duration()
                    );
                    self.locations = locations;
                    self.route_result = Some(result);
                    self.is_working = false;
                    self.current_panel = AppPanel::Results;
                    ctx.request_repaint();
                }
                WorkerMsg::Error(e) => {
                    self.status_msg = format!("Error: {e}");
                    self.is_working = false;
                    ctx.request_repaint();
                }
                WorkerMsg::LocationFound(loc) => {
                    let label = format!("📍 {:.5}, {:.5}", loc.lat.unwrap_or(0.0), loc.lng.unwrap_or(0.0));
                    self.stop_list.insert(0, label);
                    self.status_msg = "Current location added as first stop.".into();
                    ctx.request_repaint();
                }
                WorkerMsg::TileLoaded { bytes, .. } => {
                    if let Ok(img) = image::load_from_memory(&bytes) {
                        let size = [img.width() as usize, img.height() as usize];
                        let rgba = img.to_rgba8();
                        let pixels: Vec<_> = rgba.pixels()
                            .map(|p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
                            .collect();
                        let color_img = egui::ColorImage { size, pixels };
                        self.preview_texture = Some(ctx.load_texture(
                            "map_preview",
                            color_img,
                            egui::TextureOptions::LINEAR,
                        ));
                        ctx.request_repaint();
                    }
                }
            }
        }
    }
}

impl eframe::App for OnTrackApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker(ctx);
        self.top_bar(ctx);

        match self.current_panel {
            AppPanel::NewRoute => self.panel_new_route(ctx),
            AppPanel::Results  => self.panel_results(ctx),
            AppPanel::Settings => self.panel_settings(ctx),
        }
    }
}

fn main() -> eframe::Result<()> {
    let _ = dotenvy::dotenv(); // load .env before anything else

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    eframe::run_native(
        "OnTrack — TDS Field Route Optimizer",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1200.0, 780.0])
                .with_min_inner_size([900.0, 600.0])
                .with_title("OnTrack — TDS Field Route Optimizer"),
            ..Default::default()
        },
        Box::new(|cc| Box::new(OnTrackApp::new(cc))),
    )
}
