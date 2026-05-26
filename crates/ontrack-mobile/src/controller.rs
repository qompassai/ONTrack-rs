// /qompassai/ontrack-rs/crates/ontrack-mobile/src/controller.rs
// Qompass AI — OnTrack mobile: glue between Slint UI and ontrack_core
// Copyright (C) 2026 Qompass AI, All rights reserved.
// --------------------------------------------------------------------
//! Wires Slint callbacks to `ontrack_core` logic.
//!
//! All long-running work (geocoding, distance matrix, solve) runs on a
//! Slint background `Timer` thread via `slint::spawn_local` so the UI
//! stays responsive on lower-spec Android devices.

use std::rc::Rc;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::{AppWindow, StopItem};
use ontrack_core::config::Settings;
use ontrack_core::matrix::Backend;

#[derive(Default, Clone)]
struct Shared {
    addresses: Vec<String>,
    settings: Settings,
}

pub fn wire(ui: &AppWindow) -> Result<()> {
    let shared = Arc::new(Mutex::new(Shared {
        settings: Settings::from_env(),
        ..Shared::default()
    }));

    // Reflect initial settings into UI fields.
    {
        let s = shared.lock().unwrap();
        ui.set_google_maps_api_key(s.settings.google_maps_api_key.clone().into());
        ui.set_osrm_base_url(s.settings.osrm_base_url.clone().into());
        ui.set_arcgis_item_id(s.settings.arcgis_item_id.clone().into());
    }

    // ── add-stop ────────────────────────────────────────────────────────
    {
        let ui_weak = ui.as_weak();
        let shared = shared.clone();
        ui.on_add_stop(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let text = ui.get_new_stop_text();
            let s = text.trim().to_string();
            if s.is_empty() {
                return;
            }
            shared.lock().unwrap().addresses.push(s);
            ui.set_new_stop_text(SharedString::new());
            refresh_stops(&ui, &shared.lock().unwrap().addresses);
        });
    }

    // ── remove-stop ─────────────────────────────────────────────────────
    {
        let ui_weak = ui.as_weak();
        let shared = shared.clone();
        ui.on_remove_stop(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let mut s = shared.lock().unwrap();
            let i = idx as usize;
            if i < s.addresses.len() {
                s.addresses.remove(i);
            }
            refresh_stops(&ui, &s.addresses);
        });
    }

    // ── current location ───────────────────────────────────────────────
    {
        let ui_weak = ui.as_weak();
        let shared = shared.clone();
        ui.on_use_current_location(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let loc = current_location();
            if let Some(loc) = loc {
                shared.lock().unwrap().addresses.insert(0, loc.address);
                refresh_stops(&ui, &shared.lock().unwrap().addresses);
            } else {
                ui.set_status_text("Could not determine current location".into());
            }
        });
    }

    // ── optimize ────────────────────────────────────────────────────────
    {
        let ui_weak = ui.as_weak();
        let shared = shared.clone();
        ui.on_optimize_route(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            ui.set_busy(true);
            ui.set_status_text("Geocoding addresses…".into());

            let backend_idx = ui.get_backend_index();
            let backend = match backend_idx {
                1 => Backend::Google,
                2 => Backend::Haversine,
                _ => Backend::Osrm,
            };

            // Update settings snapshot from UI.
            {
                let mut s = shared.lock().unwrap();
                s.settings.google_maps_api_key = ui.get_google_maps_api_key().to_string();
                s.settings.osrm_base_url = ui.get_osrm_base_url().to_string();
                s.settings.arcgis_item_id = ui.get_arcgis_item_id().to_string();
            }

            let addresses = shared.lock().unwrap().addresses.clone();
            let settings = shared.lock().unwrap().settings.clone();
            let ui_weak2 = ui.as_weak();

            std::thread::spawn(move || {
                let key = if settings.google_maps_api_key.is_empty() {
                    None
                } else {
                    Some(settings.google_maps_api_key.as_str())
                };
                let locs = ontrack_core::geocoder::geocode_addresses(
                    &addresses, false, key, None,
                );
                let matrix = ontrack_core::matrix::build_distance_matrix(
                    &locs,
                    backend,
                    Some(&settings.osrm_base_url),
                    Some(&settings.google_maps_api_key),
                );
                let result = match matrix {
                    Ok(m) => {
                        let resolved: Vec<_> = locs.iter().filter(|l| l.is_resolved()).cloned().collect();
                        ontrack_core::solver::solve_tsp(
                            &resolved,
                            &m,
                            ontrack_core::solver::SolverConfig::default(),
                        )
                    }
                    Err(e) => Err(e),
                };

                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak2.upgrade() {
                        ui.set_busy(false);
                        match result {
                            Ok(r) => {
                                ui.set_result_summary(
                                    format!(
                                        "Total drive time: {}   ·   {}",
                                        ontrack_core::exporter::format_duration(r.total_duration_seconds),
                                        r.backend_used,
                                    )
                                    .into(),
                                );
                                let items: Vec<StopItem> = r
                                    .ordered_addresses
                                    .iter()
                                    .enumerate()
                                    .map(|(i, a)| StopItem {
                                        label: format!("{}.", i + 1).into(),
                                        address: a.clone().into(),
                                    })
                                    .collect();
                                ui.set_ordered_stops(ModelRc::new(VecModel::from(items)));
                                ui.set_status_text("Done".into());
                            }
                            Err(e) => {
                                ui.set_status_text(format!("Error: {e}").into());
                            }
                        }
                    }
                });
            });
        });
    }

    // ── open maps URL ──────────────────────────────────────────────────
    {
        let ui_weak = ui.as_weak();
        ui.on_open_maps(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let items = ui.get_ordered_stops();
            let addrs: Vec<String> = (0..items.row_count())
                .filter_map(|i| items.row_data(i).map(|s| s.address.to_string()))
                .collect();
            let url = ontrack_core::exporter::build_maps_url(&addrs);
            open_url(&url);
        });
    }

    // ── export CSV (Android: app-private files; desktop: /tmp) ─────────
    {
        let ui_weak = ui.as_weak();
        ui.on_export_csv(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let items = ui.get_ordered_stops();
            let addrs: Vec<String> = (0..items.row_count())
                .filter_map(|i| items.row_data(i).map(|s| s.address.to_string()))
                .collect();
            let dir = export_dir();
            let path = dir.join("ontrack_route.csv");
            if let Err(e) = ontrack_core::exporter::export_csv(&addrs, &path) {
                ui.set_status_text(format!("Export failed: {e}").into());
            } else {
                ui.set_status_text(format!("Exported to {}", path.display()).into());
            }
        });
    }

    // ── save settings ──────────────────────────────────────────────────
    {
        let ui_weak = ui.as_weak();
        let shared = shared.clone();
        ui.on_save_settings(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let mut s = shared.lock().unwrap();
            s.settings.google_maps_api_key = ui.get_google_maps_api_key().to_string();
            s.settings.osrm_base_url = ui.get_osrm_base_url().to_string();
            s.settings.arcgis_item_id = ui.get_arcgis_item_id().to_string();
            ui.set_status_text("Settings saved (in-memory)".into());
        });
    }

    // ── voice (stubbed unless built with `voice` feature) ──────────────
    {
        let ui_weak = ui.as_weak();
        ui.on_start_voice(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_status_text("Voice capture requires the `voice` feature.".into());
            }
        });
        let ui_weak = ui.as_weak();
        ui.on_stop_voice(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_status_text("Voice capture stopped.".into());
            }
        });
    }

    Ok(())
}

fn refresh_stops(ui: &AppWindow, addresses: &[String]) {
    let items: Vec<StopItem> = addresses
        .iter()
        .enumerate()
        .map(|(i, a)| StopItem {
            label: format!("{}.", i + 1).into(),
            address: a.clone().into(),
        })
        .collect();
    ui.set_stops(ModelRc::new(VecModel::from(items)));
}

fn current_location() -> Option<ontrack_core::geocoder::Location> {
    #[cfg(target_os = "android")]
    {
        if let Some(loc) = crate::gps::last_known() {
            return Some(loc);
        }
    }
    ontrack_core::geocoder::get_current_location()
}

#[allow(unused_variables)]
fn open_url(url: &str) {
    #[cfg(target_os = "android")]
    {
        let _ = open_url_android(url);
    }
    // Best-effort cross-platform launcher.
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn();
    }
}

#[cfg(target_os = "android")]
fn open_url_android(url: &str) -> anyhow::Result<()> {
    use jni::objects::{JObject, JValue};
    use jni::JavaVM;

    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm() as *mut _) }?;
    let mut env = vm.attach_current_thread()?;
    let activity = unsafe { JObject::from_raw(ctx.context() as jni::sys::jobject) };

    let uri_class = env.find_class("android/net/Uri")?;
    let url_string = env.new_string(url)?;
    let uri = env
        .call_static_method(
            uri_class,
            "parse",
            "(Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&JObject::from(url_string))],
        )?
        .l()?;

    let intent_class = env.find_class("android/content/Intent")?;
    let action = env.new_string("android.intent.action.VIEW")?;
    let intent = env.new_object(
        intent_class,
        "(Ljava/lang/String;Landroid/net/Uri;)V",
        &[JValue::Object(&JObject::from(action)), JValue::Object(&uri)],
    )?;
    env.call_method(
        &intent,
        "addFlags",
        "(I)Landroid/content/Intent;",
        &[JValue::Int(0x10000000)], // FLAG_ACTIVITY_NEW_TASK
    )?;
    env.call_method(
        activity,
        "startActivity",
        "(Landroid/content/Intent;)V",
        &[JValue::Object(&intent)],
    )?;
    Ok(())
}

fn export_dir() -> std::path::PathBuf {
    #[cfg(target_os = "android")]
    {
        if let Some(dir) = android_files_dir() {
            return dir;
        }
    }
    std::env::temp_dir()
}

#[cfg(target_os = "android")]
fn android_files_dir() -> Option<std::path::PathBuf> {
    use jni::objects::{JObject, JString};
    use jni::JavaVM;

    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm() as *mut _) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { JObject::from_raw(ctx.context() as jni::sys::jobject) };
    let file = env
        .call_method(activity, "getFilesDir", "()Ljava/io/File;", &[])
        .ok()?
        .l()
        .ok()?;
    let path: JString = env
        .call_method(file, "getAbsolutePath", "()Ljava/lang/String;", &[])
        .ok()?
        .l()
        .ok()?
        .into();
    let s: String = env.get_string(&path).ok()?.into();
    Some(std::path::PathBuf::from(s))
}

#[allow(dead_code)]
fn _suppress(_: Rc<()>) {}
