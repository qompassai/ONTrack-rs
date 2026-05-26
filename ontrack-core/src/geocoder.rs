//! geocoder.rs — Address → lat/lng using Nominatim (free) or Google Geocoding API.
//! Also provides current-location detection via IP geolocation.

use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;
use tracing::warn;

/// A geocoded location — mirrors the Python `dict` with address/lat/lng.
#[derive(Debug, Clone)]
pub struct Location {
    pub address: String,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
}

impl Location {
    pub fn is_resolved(&self) -> bool {
        self.lat.is_some() && self.lng.is_some()
    }

    /// Return (lat, lng) — panics if unresolved. Check `is_resolved()` first.
    pub fn coords(&self) -> (f64, f64) {
        (self.lat.unwrap(), self.lng.unwrap())
    }
}

// ── Nominatim response ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct NominatimResult {
    lat: String,
    lon: String,
}

// ── Google Geocoding response ──────────────────────────────────────────────

#[derive(Deserialize)]
struct GoogleGeocodeResponse {
    status: String,
    results: Vec<GoogleGeocodeResult>,
}

#[derive(Deserialize)]
struct GoogleGeocodeResult {
    geometry: GoogleGeometry,
}

#[derive(Deserialize)]
struct GoogleGeometry {
    location: LatLng,
}

#[derive(Deserialize)]
struct LatLng {
    lat: f64,
    lng: f64,
}

// ── IP geolocation response ────────────────────────────────────────────────

#[derive(Deserialize)]
struct IpApiResponse {
    status: String,
    lat: f64,
    lon: f64,
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Geocode a single address via Nominatim (free, no key).
pub async fn geocode_nominatim(client: &Client, address: &str) -> Location {
    let result: Result<Location> = async {
        let resp = client
            .get("https://nominatim.openstreetmap.org/search")
            .query(&[
                ("q", address),
                ("format", "json"),
                ("limit", "1"),
                ("addressdetails", "0"),
            ])
            .header("User-Agent", "OnTrack-TDS/2.0 (field route optimizer)")
            .send()
            .await?
            .error_for_status()?;

        let results: Vec<NominatimResult> = resp.json().await?;

        if let Some(r) = results.into_iter().next() {
            let lat: f64 = r.lat.parse()?;
            let lng: f64 = r.lon.parse()?;
            Ok(Location { address: address.to_string(), lat: Some(lat), lng: Some(lng) })
        } else {
            Ok(Location { address: address.to_string(), lat: None, lng: None })
        }
    }
    .await;

    match result {
        Ok(loc) => loc,
        Err(e) => {
            warn!("Nominatim geocode failed for {:?}: {}", address, e);
            Location { address: address.to_string(), lat: None, lng: None }
        }
    }
}

/// Geocode a single address via Google Geocoding API (requires key).
pub async fn geocode_google(client: &Client, address: &str, api_key: &str) -> Location {
    let result: Result<Location> = async {
        let resp = client
            .get("https://maps.googleapis.com/maps/api/geocode/json")
            .query(&[("address", address), ("key", api_key)])
            .send()
            .await?
            .error_for_status()?;

        let data: GoogleGeocodeResponse = resp.json().await?;

        if data.status == "OK" {
            if let Some(r) = data.results.into_iter().next() {
                return Ok(Location {
                    address: address.to_string(),
                    lat: Some(r.geometry.location.lat),
                    lng: Some(r.geometry.location.lng),
                });
            }
        }
        Ok(Location { address: address.to_string(), lat: None, lng: None })
    }
    .await;

    match result {
        Ok(loc) => loc,
        Err(e) => {
            warn!("Google geocode failed for {:?}: {}", address, e);
            Location { address: address.to_string(), lat: None, lng: None }
        }
    }
}

/// Geocode a list of addresses, reporting progress via an optional callback.
///
/// # Arguments
/// * `addresses`   — list of address strings
/// * `use_google`  — if true and `api_key` is set, use Google Geocoding
/// * `api_key`     — Google Maps API key (ignored if `use_google` is false)
/// * `progress_cb` — optional `FnMut(done, total)` called after each geocode
pub async fn geocode_addresses(
    client: &Client,
    addresses: &[String],
    use_google: bool,
    api_key: Option<&str>,
    mut progress_cb: impl FnMut(usize, usize),
) -> Vec<Location> {
    let total = addresses.len();
    let mut results = Vec::with_capacity(total);

    for (i, addr) in addresses.iter().enumerate() {
        let loc = if use_google {
            if let Some(key) = api_key {
                geocode_google(client, addr, key).await
            } else {
                geocode_nominatim(client, addr).await
            }
        } else {
            geocode_nominatim(client, addr).await
        };

        // Nominatim rate limit: 1 request/second per their ToS
        if !use_google {
            tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        }

        results.push(loc);
        progress_cb(i + 1, total);
    }

    results
}

/// Get current location via IP geolocation (desktop fallback, no API key).
pub async fn get_current_location_ip(client: &Client) -> Option<Location> {
    let resp = client
        .get("http://ip-api.com/json/?fields=lat,lon,status")
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?;

    let data: IpApiResponse = resp.json().await.ok()?;

    if data.status == "success" {
        Some(Location {
            address: "Current Location".to_string(),
            lat: Some(data.lat),
            lng: Some(data.lon),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_is_resolved() {
        let loc = Location { address: "test".into(), lat: Some(47.6), lng: Some(-117.4) };
        assert!(loc.is_resolved());
        let loc2 = Location { address: "test".into(), lat: None, lng: None };
        assert!(!loc2.is_resolved());
    }

    #[test]
    fn location_coords() {
        let loc = Location { address: "test".into(), lat: Some(47.6), lng: Some(-117.4) };
        assert_eq!(loc.coords(), (47.6, -117.4));
    }
}
