//! config.rs — Runtime configuration loaded from environment / .env

use std::env;

/// Global app configuration.
/// Load with `Config::from_env()` once at startup.
#[derive(Debug, Clone)]
pub struct Config {
    pub google_maps_api_key: Option<String>,
    pub osrm_base_url: String,
    pub arcgis_item_id: Option<String>,
    pub app_version: &'static str,
    pub app_name: &'static str,
    pub org_name: &'static str,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            google_maps_api_key: None,
            osrm_base_url: "http://router.project-osrm.org".to_string(),
            arcgis_item_id: None,
            app_version: "2.0.0",
            app_name: "OnTrack",
            org_name: "TDS Telecom",
        }
    }
}

impl Config {
    /// Load from .env file (if present) then environment variables.
    pub fn from_env() -> Self {
        // Load .env — silently ignore if missing (production / Android)
        let _ = dotenvy::dotenv();

        Self {
            google_maps_api_key: env::var("GOOGLE_MAPS_API_KEY").ok().filter(|s| !s.is_empty()),
            osrm_base_url: env::var("OSRM_BASE_URL")
                .unwrap_or_else(|_| "http://router.project-osrm.org".to_string()),
            arcgis_item_id: env::var("ARCGIS_ITEM_ID").ok().filter(|s| !s.is_empty()),
            ..Default::default()
        }
    }

    pub fn has_google_key(&self) -> bool {
        self.google_maps_api_key.is_some()
    }
}
