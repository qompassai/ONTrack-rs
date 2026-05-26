// /qompassai/ontrack-rs/crates/ontrack-core/src/geocoder.rs
// Qompass AI — OnTrack core: address geocoding
// Copyright (C) 2026 Qompass AI, All rights reserved.
// -----------------------------------------------------
//! Address geocoding and current-location detection.
//!
//! Backends:
//!   - **Nominatim** — free, default, no API key
//!   - **Google Geocoding API** — optional, better rural accuracy
//!
//! Current location:
//!   - Desktop: IP-based via `ip-api.com` (no perms required)
//!   - Android: GPS via JNI (delegated to host code in `ontrack-mobile`)

use anyhow::{anyhow, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A geocoded address with optional coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub address: String,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
}

impl Location {
    pub fn new(address: impl Into<String>) -> Self {
        Self { address: address.into(), lat: None, lng: None }
    }
    pub fn is_resolved(&self) -> bool {
        self.lat.is_some() && self.lng.is_some()
    }
}

fn http_client() -> Result<Client> {
    Client::builder()
        .user_agent("ontrack-rs/2.0 (TDS Telecom field router)")
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| anyhow!("http client build: {e}"))
}

// ── Nominatim ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct NominatimHit {
    lat: String,
    lon: String,
}

/// Geocode a single address via OpenStreetMap Nominatim.
pub fn geocode_address_nominatim(addr: &str) -> Result<Location> {
    let client = http_client()?;
    let resp: Vec<NominatimHit> = client
        .get("https://nominatim.openstreetmap.org/search")
        .query(&[("q", addr), ("format", "json"), ("limit", "1")])
        .send()?
        .error_for_status()?
        .json()?;

    let mut loc = Location::new(addr);
    if let Some(hit) = resp.first() {
        loc.lat = hit.lat.parse().ok();
        loc.lng = hit.lon.parse().ok();
    }
    Ok(loc)
}

// ── Google Geocoding ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GoogleGeoResp {
    status: String,
    results: Vec<GoogleGeoResult>,
}
#[derive(Deserialize)]
struct GoogleGeoResult {
    geometry: GoogleGeoGeom,
}
#[derive(Deserialize)]
struct GoogleGeoGeom {
    location: LatLng,
}
#[derive(Deserialize)]
struct LatLng {
    lat: f64,
    lng: f64,
}

/// Geocode a single address via Google Geocoding API.
pub fn geocode_address_google(addr: &str, api_key: &str) -> Result<Location> {
    let client = http_client()?;
    let resp: GoogleGeoResp = client
        .get("https://maps.googleapis.com/maps/api/geocode/json")
        .query(&[("address", addr), ("key", api_key)])
        .send()?
        .error_for_status()?
        .json()?;

    let mut loc = Location::new(addr);
    if resp.status == "OK" {
        if let Some(r) = resp.results.first() {
            loc.lat = Some(r.geometry.location.lat);
            loc.lng = Some(r.geometry.location.lng);
        }
    }
    Ok(loc)
}

/// Geocode a list of addresses sequentially.
///
/// `progress` is called with `(done, total)` after each address. Errors on
/// individual addresses are swallowed — those entries remain unresolved.
pub fn geocode_addresses(
    addresses: &[String],
    use_google: bool,
    google_api_key: Option<&str>,
    mut progress: Option<&mut dyn FnMut(usize, usize)>,
) -> Vec<Location> {
    let key = google_api_key.unwrap_or("");
    let total = addresses.len();
    let mut out = Vec::with_capacity(total);

    for (i, addr) in addresses.iter().enumerate() {
        let result = if use_google && !key.is_empty() {
            geocode_address_google(addr, key)
        } else {
            geocode_address_nominatim(addr)
        };
        out.push(result.unwrap_or_else(|_| Location::new(addr.clone())));
        if let Some(cb) = progress.as_deref_mut() {
            cb(i + 1, total);
        }
    }
    out
}

// ── Current location ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct IpApiResp {
    status: String,
    lat: Option<f64>,
    lon: Option<f64>,
}

/// Coarse current-location lookup via IP geolocation (desktop fallback).
///
/// On Android, callers should provide a GPS-derived `Location` directly
/// (see `ontrack-mobile`'s `gps` module).
pub fn get_current_location() -> Option<Location> {
    let client = http_client().ok()?;
    let resp: IpApiResp = client
        .get("http://ip-api.com/json/?fields=lat,lon,status")
        .send()
        .ok()?
        .json()
        .ok()?;
    if resp.status == "success" {
        Some(Location {
            address: "Current Location".to_string(),
            lat: resp.lat,
            lng: resp.lon,
        })
    } else {
        None
    }
}
