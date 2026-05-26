
use eframe::{egui, App, CreationContext, Frame};
use ontrack_core::config::Settings;
use ontrack_core::geocoder::Location;
use ontrack_core::matrix::Backend;
use ontrack_core::solver::RouteResult;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::views;

#[derive(Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Home,
    Results,
    Settings,
}

#[derive(Default)]
pub struct WorkerState {
    pub busy: bool,
    pub progress: (usize, usize),
    pub status_line: String,
    pub error: Option<String>,
    pub result: Option<RouteResult>,
    pub locations: Vec<Location>,
}

pub struct OnTrackApp {
    pub view: View,
    pub addresses: Vec<String>,
    pub address_input: String,
    pub backend: Backend,
    pub use_google: bool,
    pub settings: Settings,
    pub worker: Arc<Mutex<WorkerState>>,
}

impl OnTrackApp {
    pub fn new(_cc: &CreationContext<'_>) -> Self {
        Self {
            view: View::Home,
            addresses: Vec::new(),
            address_input: String::new(),
            backend: Backend::Osrm,
            use_google: false,
            settings: Settings::from_env(),
            worker: Arc::new(Mutex::new(WorkerState::default())),
        }
    }

    pub fn run_optimize(&self) {
        let addresses = self.addresses.clone();
        let backend = self.backend;
        let use_google = self.use_google;
        let settings = self.settings.clone();
        let worker = self.worker.clone();

        {
            let mut w = worker.lock().unwrap();
            w.busy = true;
            w.progress = (0, addresses.len());
            w.error = None;
            w.status_line = "Geocoding addresses…".to_string();
            w.result = None;
        }

        thread::spawn(move || {
            let key = if settings.google_maps_api_key.is_empty() {
                None
            } else {
                Some(settings.google_maps_api_key.as_str())
            };
            let mut progress_cb = {
                let worker = worker.clone();
                move |done: usize, total: usize| {
                    let mut w = worker.lock().unwrap();
                    w.progress = (done, total);
                }
            };
            let locs = ontrack_core::geocoder::geocode_addresses(
                &addresses,
                use_google,
                key,
                Some(&mut progress_cb),
            );

            {
                let mut w = worker.lock().unwrap();
                w.status_line = "Building distance matrix…".to_string();
            }

            let matrix_res = ontrack_core::matrix::build_distance_matrix(
                &locs,
                backend,
                Some(&settings.osrm_base_url),
                Some(&settings.google_maps_api_key),
            );

            let matrix = match matrix_res {
                Ok(m) => m,
                Err(e) => {
                    let mut w = worker.lock().unwrap();
                    w.busy = false;
                    w.error = Some(format!("matrix: {e}"));
                    return;
                }
            };

            {
                let mut w = worker.lock().unwrap();
                w.status_line = "Solving route…".to_string();
            }

            let resolved: Vec<Location> =
                locs.iter().filter(|l| l.is_resolved()).cloned().collect();
            let cfg = ontrack_core::solver::SolverConfig::default();
            match ontrack_core::solver::solve_tsp(&resolved, &matrix, cfg) {
                Ok(r) => {
                    let mut w = worker.lock().unwrap();
                    w.busy = false;
                    w.result = Some(r);
                    w.locations = resolved;
                    w.status_line = "Done".to_string();
                }
                Err(e) => {
                    let mut w = worker.lock().unwrap();
                    w.busy = false;
                    w.error = Some(format!("solve: {e}"));
                }
            }
        });
    }
}

impl App for OnTrackApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        egui::TopBottomPanel::top("nav").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("OnTrack");
                ui.separator();
                ui.selectable_value(&mut self.view, View::Home, "Home");
                ui.selectable_value(&mut self.view, View::Results, "Results");
                ui.selectable_value(&mut self.view, View::Settings, "Settings");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("v{}", ontrack_core::config::APP_VERSION));
                });
            });
        });

        let (busy, has_result) = {
            let w = self.worker.lock().unwrap();
            (w.busy, w.result.is_some())
        };
        if !busy && has_result && self.view == View::Home {
            self.view = View::Results;
        }

        egui::CentralPanel::default().show(ctx, |ui| match self.view {
            View::Home => views::home::ui(self, ui),
            View::Results => views::results::ui(self, ui),
            View::Settings => views::settings::ui(self, ui),
        });

        let busy = self.worker.lock().unwrap().busy;
        if busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}
