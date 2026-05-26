//! matrix.rs — Build an NxN duration/distance matrix from geocoded locations.
//!
//! Backends:
//!   • `Haversine` — straight-line great-circle distance (meters), no network
//!   • `Osrm`      — real road durations (seconds) via OSRM Table API
//!   • `Google`    — real road durations (seconds) via Google Distance Matrix API

use crate::geocoder::Location;
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use std::f64::consts::PI;
use tracing::debug;

pub type Matrix = Vec<Vec<f64>>;

/// Which backend to use for building the distance matrix.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MatrixBackend {
    #[default]
    Osrm,
    Google,
    Haversine,
}

impl std::str::FromStr for MatrixBackend {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "osrm" => Ok(Self::Osrm),
            "google" => Ok(Self::Google),
            "haversine" => Ok(Self::Haversine),
            other => Err(anyhow!("Unknown matrix backend: {other}")),
        }
    }
}

// ── Haversine ──────────────────────────────────────────────────────────────

/// Great-circle distance in meters between two lat/lng points.
pub fn haversine(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    const R: f64 = 6_371_000.0;
    let phi1 = lat1 * PI / 180.0;
    let phi2 = lat2 * PI / 180.0;
    let dphi = (lat2 - lat1) * PI / 180.0;
    let dlam = (lng2 - lng1) * PI / 180.0;
    let a = (dphi / 2.0).sin().powi(2)
        + phi1.cos() * phi2.cos() * (dlam / 2.0).sin().powi(2);
    R * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())
}

fn haversine_matrix(locations: &[Location]) -> Matrix {
    let n = locations.len();
    (0..n)
        .map(|i| {
            let (lat1, lng1) = locations[i].coords();
            (0..n)
                .map(|j| {
                    if i == j {
                        0.0
                    } else {
                        let (lat2, lng2) = locations[j].coords();
                        haversine(lat1, lng1, lat2, lng2)
                    }
                })
                .collect()
        })
        .collect()
}

// ── OSRM ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct OsrmTableResponse {
    code: String,
    durations: Option<Vec<Vec<Option<f64>>>>,
    #[serde(default)]
    message: String,
}

async fn osrm_matrix(client: &Client, locations: &[Location], base_url: &str) -> Result<Matrix> {
    let coords: String = locations
        .iter()
        .map(|l| {
            let (lat, lng) = l.coords();
            format!("{},{}", lng, lat)
        })
        .collect::<Vec<_>>()
        .join(";");

    let url = format!("{base_url}/table/v1/driving/{coords}");
    debug!("OSRM request: {url}");

    let resp = client
        .get(&url)
        .query(&[("annotations", "duration")])
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?
        .error_for_status()?;

    let data: OsrmTableResponse = resp.json().await?;

    if data.code != "Ok" {
        return Err(anyhow!("OSRM error: {}", data.message));
    }

    let raw = data
        .durations
        .ok_or_else(|| anyhow!("OSRM response missing 'durations' field"))?;

    // Replace null values (unreachable pairs) with haversine fallback
    let matrix: Matrix = raw
        .iter()
        .enumerate()
        .map(|(i, row)| {
            row.iter()
                .enumerate()
                .map(|(j, val)| {
                    val.unwrap_or_else(|| {
                        let (lat1, lng1) = locations[i].coords();
                        let (lat2, lng2) = locations[j].coords();
                        haversine(lat1, lng1, lat2, lng2)
                    })
                })
                .collect()
        })
        .collect();

    Ok(matrix)
}

// ── Google Distance Matrix ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct GoogleMatrixResponse {
    status: String,
    rows: Vec<GoogleMatrixRow>,
}

#[derive(Deserialize)]
struct GoogleMatrixRow {
    elements: Vec<GoogleMatrixElement>,
}

#[derive(Deserialize)]
struct GoogleMatrixElement {
    status: String,
    duration: Option<GoogleValue>,
}

#[derive(Deserialize)]
struct GoogleValue {
    value: f64,
}

async fn google_matrix(
    client: &Client,
    locations: &[Location],
    api_key: &str,
) -> Result<Matrix> {
    let n = locations.len();
    let mut matrix = vec![vec![0.0f64; n]; n];
    let batch = 10usize;

    for i in (0..n).step_by(batch) {
        let origins: String = locations[i..n.min(i + batch)]
            .iter()
            .map(|l| l.address.clone())
            .collect::<Vec<_>>()
            .join("|");

        for j in (0..n).step_by(batch) {
            let dests: String = locations[j..n.min(j + batch)]
                .iter()
                .map(|l| l.address.clone())
                .collect::<Vec<_>>()
                .join("|");

            let resp = client
                .get("https://maps.googleapis.com/maps/api/distancematrix/json")
                .query(&[
                    ("origins", origins.as_str()),
                    ("destinations", dests.as_str()),
                    ("key", api_key),
                ])
                .timeout(std::time::Duration::from_secs(30))
                .send()
                .await?
                .error_for_status()?;

            let data: GoogleMatrixResponse = resp.json().await?;
            if data.status != "OK" {
                return Err(anyhow!("Google Distance Matrix API error: {}", data.status));
            }

            for (ri, row) in data.rows.iter().enumerate() {
                for (ci, elem) in row.elements.iter().enumerate() {
                    matrix[i + ri][j + ci] = if elem.status == "OK" {
                        elem.duration.as_ref().map(|d| d.value).unwrap_or(0.0)
                    } else {
                        let (lat1, lng1) = locations[i + ri].coords();
                        let (lat2, lng2) = locations[j + ci].coords();
                        haversine(lat1, lng1, lat2, lng2)
                    };
                }
            }
        }
    }

    Ok(matrix)
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Build an NxN distance/duration matrix from geocoded locations.
///
/// Filters out unresolved (lat/lng = None) locations before building.
/// Returns `(resolved_locations, matrix)`.
pub async fn build_distance_matrix(
    client: &Client,
    locations: &[Location],
    backend: &MatrixBackend,
    osrm_url: &str,
    google_api_key: Option<&str>,
) -> Result<(Vec<Location>, Matrix)> {
    let resolved: Vec<Location> = locations
        .iter()
        .filter(|l| l.is_resolved())
        .cloned()
        .collect();

    if resolved.is_empty() {
        return Err(anyhow!("No geocoded locations available to build matrix."));
    }

    let matrix = match backend {
        MatrixBackend::Haversine => haversine_matrix(&resolved),
        MatrixBackend::Osrm => osrm_matrix(client, &resolved, osrm_url).await?,
        MatrixBackend::Google => {
            let key = google_api_key
                .ok_or_else(|| anyhow!("Google backend requires an API key"))?;
            google_matrix(client, &resolved, key).await?
        }
    };

    Ok((resolved, matrix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haversine_same_point() {
        assert_eq!(haversine(47.0, -117.0, 47.0, -117.0), 0.0);
    }

    #[test]
    fn haversine_symmetry() {
        let d1 = haversine(47.6588, -117.4260, 47.6777, -116.7805);
        let d2 = haversine(47.6777, -116.7805, 47.6588, -117.4260);
        assert!((d1 - d2).abs() < 1e-6);
    }

    #[test]
    fn haversine_spokane_cda() {
        // Spokane WA to Coeur d'Alene ID — ~48 km great-circle
        let d = haversine(47.6588, -117.4260, 47.6777, -116.7805);
        assert!(d > 45_000.0 && d < 55_000.0, "Expected ~48 km, got {d:.0} m");
    }

    #[test]
    fn haversine_matrix_diagonal_zero() {
        let locs = vec![
            Location { address: "A".into(), lat: Some(47.6588), lng: Some(-117.4260) },
            Location { address: "B".into(), lat: Some(47.6601), lng: Some(-117.4200) },
        ];
        let m = haversine_matrix(&locs);
        assert_eq!(m[0][0], 0.0);
        assert_eq!(m[1][1], 0.0);
        assert!((m[0][1] - m[1][0]).abs() < 1e-6);
    }
}
