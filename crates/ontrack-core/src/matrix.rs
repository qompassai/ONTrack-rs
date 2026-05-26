
use anyhow::{anyhow, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::time::Duration;

use crate::config::OSRM_PUBLIC;
use crate::geocoder::Location;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Osrm,
    Google,
    Haversine,
}

impl Backend {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "osrm" => Ok(Self::Osrm),
            "google" => Ok(Self::Google),
            "haversine" => Ok(Self::Haversine),
            other => Err(anyhow!("invalid backend: {other}")),
        }
    }
}

pub fn haversine(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    let r = 6_371_000.0_f64;
    let phi1 = lat1.to_radians();
    let phi2 = lat2.to_radians();
    let dphi = (lat2 - lat1).to_radians();
    let dlam = (lng2 - lng1).to_radians();
    let a = (dphi / 2.0).sin().powi(2)
        + phi1.cos() * phi2.cos() * (dlam / 2.0).sin().powi(2);
    r * 2.0 * a.sqrt().asin()
}

fn http_client() -> Result<Client> {
    Client::builder()
        .user_agent("ontrack-rs/2.0")
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| anyhow!("http client build: {e}"))
}


#[derive(Deserialize)]
struct OsrmResp {
    code: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    durations: Option<Vec<Vec<f64>>>,
}

fn osrm_matrix(locations: &[&Location], base_url: &str) -> Result<Vec<Vec<f64>>> {
    let coords: Vec<String> = locations
        .iter()
        .map(|l| format!("{},{}", l.lng.unwrap_or(0.0), l.lat.unwrap_or(0.0)))
        .collect();
    let url = format!("{}/table/v1/driving/{}", base_url, coords.join(";"));
    let resp: OsrmResp = http_client()?
        .get(&url)
        .query(&[("annotations", "duration,distance")])
        .send()?
        .error_for_status()?
        .json()?;

    if resp.code != "Ok" {
        return Err(anyhow!(
            "OSRM error: {}",
            resp.message.unwrap_or_else(|| "unknown".to_string())
        ));
    }
    resp.durations
        .ok_or_else(|| anyhow!("OSRM response missing durations"))
}


#[derive(Deserialize)]
struct GoogleResp {
    status: String,
    rows: Vec<GoogleRow>,
}
#[derive(Deserialize)]
struct GoogleRow {
    elements: Vec<GoogleElem>,
}
#[derive(Deserialize)]
struct GoogleElem {
    status: String,
    #[serde(default)]
    duration: Option<GoogleDur>,
}
#[derive(Deserialize)]
struct GoogleDur {
    value: f64,
}

fn google_matrix(locations: &[&Location], api_key: &str) -> Result<Vec<Vec<f64>>> {
    let n = locations.len();
    let mut matrix = vec![vec![0.0_f64; n]; n];
    let batch = 10usize;

    for i in (0..n).step_by(batch) {
        let i_end = (i + batch).min(n);
        let origins = locations[i..i_end]
            .iter()
            .map(|l| format!("{},{}", l.lat.unwrap_or(0.0), l.lng.unwrap_or(0.0)))
            .collect::<Vec<_>>()
            .join("|");

        for j in (0..n).step_by(batch) {
            let j_end = (j + batch).min(n);
            let dests = locations[j..j_end]
                .iter()
                .map(|l| format!("{},{}", l.lat.unwrap_or(0.0), l.lng.unwrap_or(0.0)))
                .collect::<Vec<_>>()
                .join("|");

            let resp: GoogleResp = http_client()?
                .get("https://maps.googleapis.com/maps/api/distancematrix/json")
                .query(&[("origins", &origins), ("destinations", &dests), ("key", &api_key.to_string())])
                .send()?
                .error_for_status()?
                .json()?;

            if resp.status != "OK" {
                return Err(anyhow!("Google API error: {}", resp.status));
            }

            for (ri, row) in resp.rows.iter().enumerate() {
                for (ci, elem) in row.elements.iter().enumerate() {
                    let val = if elem.status == "OK" {
                        elem.duration.as_ref().map(|d| d.value).unwrap_or(0.0)
                    } else {
                        let a = locations[i + ri];
                        let b = locations[j + ci];
                        haversine(
                            a.lat.unwrap_or(0.0),
                            a.lng.unwrap_or(0.0),
                            b.lat.unwrap_or(0.0),
                            b.lng.unwrap_or(0.0),
                        )
                    };
                    matrix[i + ri][j + ci] = val;
                }
            }
        }
    }
    Ok(matrix)
}

fn haversine_matrix(locations: &[&Location]) -> Vec<Vec<f64>> {
    let n = locations.len();
    let mut m = vec![vec![0.0_f64; n]; n];
    for i in 0..n {
        for j in (i + 1)..n {
            let d = haversine(
                locations[i].lat.unwrap_or(0.0),
                locations[i].lng.unwrap_or(0.0),
                locations[j].lat.unwrap_or(0.0),
                locations[j].lng.unwrap_or(0.0),
            );
            m[i][j] = d;
            m[j][i] = d;
        }
    }
    m
}

pub fn build_distance_matrix(
    locations: &[Location],
    backend: Backend,
    osrm_url: Option<&str>,
    google_api_key: Option<&str>,
) -> Result<Vec<Vec<f64>>> {
    let resolved: Vec<&Location> = locations.iter().filter(|l| l.is_resolved()).collect();
    if resolved.is_empty() {
        return Err(anyhow!("no geocoded locations available to build matrix"));
    }

    match backend {
        Backend::Osrm => osrm_matrix(&resolved, osrm_url.unwrap_or(OSRM_PUBLIC)),
        Backend::Google => {
            let key = google_api_key
                .filter(|k| !k.is_empty())
                .ok_or_else(|| anyhow!("Google backend requires an API key"))?;
            google_matrix(&resolved, key)
        }
        Backend::Haversine => Ok(haversine_matrix(&resolved)),
    }
}
