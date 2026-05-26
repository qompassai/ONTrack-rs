// /qompassai/ontrack-rs/crates/ontrack-core/src/config.rs
// Qompass AI — OnTrack core: runtime configuration
// Copyright (C) 2026 Qompass AI, All rights reserved.
// -----------------------------------------------------
//! Runtime configuration loaded from environment variables.
//!
//! `.env` files are loaded automatically on desktop builds via `dotenvy`.
//! Android builds rely on process environment only.

use std::env;

pub const APP_NAME: &str = "OnTrack";
pub const APP_VERSION: &str = "2.0.0";
pub const ORG_NAME: &str = "TDS Telecom";

pub const OSRM_PUBLIC: &str = "http://router.project-osrm.org";

/// Runtime configuration snapshot.
#[derive(Debug, Clone)]
pub struct Settings {
    pub google_maps_api_key: String,
    pub osrm_base_url: String,
    pub arcgis_item_id: String,
    pub whisper_model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            google_maps_api_key: String::new(),
            osrm_base_url: OSRM_PUBLIC.to_string(),
            arcgis_item_id: String::new(),
            whisper_model: "base".to_string(),
        }
    }
}

impl Settings {
    /// Load settings from environment variables (and `.env` if present).
    pub fn from_env() -> Self {
        // Best-effort .env load — silently ignored on Android / when missing.
        let _ = dotenvy::dotenv();

        Self {
            google_maps_api_key: env::var("GOOGLE_MAPS_API_KEY").unwrap_or_default(),
            osrm_base_url: env::var("OSRM_BASE_URL").unwrap_or_else(|_| OSRM_PUBLIC.to_string()),
            arcgis_item_id: env::var("ARCGIS_ITEM_ID").unwrap_or_default(),
            whisper_model: env::var("ONTRACK_WHISPER_MODEL").unwrap_or_else(|_| "base".to_string()),
        }
    }
}
