//! exporter.rs — Route export and external map/navigation integration.
//!
//! Mirrors Python's core/exporter.py exactly.

use anyhow::Result;
use std::path::Path;
use urlencoding::encode;

// ── CSV export ─────────────────────────────────────────────────────────────

/// Write an ordered route to a CSV file with columns: stop, address.
pub fn export_csv<P: AsRef<Path>>(addresses: &[String], path: P) -> Result<()> {
    let mut wtr = csv::WriterBuilder::new()
        .has_headers(true)
        .from_path(path)?;

    wtr.write_record(["stop", "address"])?;
    for (i, addr) in addresses.iter().enumerate() {
        wtr.write_record([&(i + 1).to_string(), addr])?;
    }
    wtr.flush()?;
    Ok(())
}

// ── Google Maps URLs ───────────────────────────────────────────────────────

/// Build a Google Maps Directions URL that works in browser and launches the
/// Maps app on Android/iOS.  Supports up to 10 total stops (9 waypoints).
pub fn build_maps_url(addresses: &[String]) -> String {
    if addresses.is_empty() {
        return "https://www.google.com/maps/dir/?api=1".to_string();
    }
    if addresses.len() == 1 {
        return format!(
            "https://www.google.com/maps/dir/?api=1&destination={}&travelmode=driving",
            encode(&addresses[0])
        );
    }

    let origin = encode(&addresses[0]);
    let destination = encode(addresses.last().unwrap());
    let mut url = format!(
        "https://www.google.com/maps/dir/?api=1&origin={origin}&destination={destination}&travelmode=driving"
    );

    let waypoints = &addresses[1..addresses.len() - 1];
    if !waypoints.is_empty() {
        let wp = waypoints
            .iter()
            .map(|a| encode(a).into_owned())
            .collect::<Vec<_>>()
            .join("%7C"); // URL-encoded "|"
        url.push_str(&format!("&waypoints={wp}"));
    }

    url
}

/// For routes with > 10 stops, split into multiple Maps URLs (max 10 per URL).
pub fn build_maps_url_chunked(addresses: &[String]) -> Vec<String> {
    addresses
        .chunks(10)
        .map(|chunk| build_maps_url(chunk))
        .collect()
}

// ── Google Street View Static API ─────────────────────────────────────────

/// Build a Street View Static API image URL.
/// Returns a URL that resolves to a JPEG image when fetched.
/// Requires a Google Maps API key with Street View Static API enabled.
pub fn build_streetview_url(
    lat: f64,
    lng: f64,
    api_key: &str,
    width: u32,
    height: u32,
) -> String {
    format!(
        "https://maps.googleapis.com/maps/api/streetview\
         ?size={width}x{height}&location={lat},{lng}&fov=90&pitch=0&key={api_key}"
    )
}

/// Free Street View embed URL — opens interactive panorama in browser.
/// No API key required.
pub fn build_streetview_embed_url(lat: f64, lng: f64) -> String {
    format!(
        "https://www.google.com/maps/@{lat},{lng},3a,90y,0h,90t/data=!3m4!1e1!3m2!1s!2e0"
    )
}

// ── OpenStreetMap tile URL ─────────────────────────────────────────────────

/// Build a free OSM tile URL for the given coordinates (zoom 16 = street level).
/// No API key required. Usable as an <img src> or in any HTTP image loader.
pub fn build_osm_tile_url(lat: f64, lng: f64, zoom: u32) -> String {
    use std::f64::consts::PI;
    let n = (2u64.pow(zoom)) as f64;
    let x = ((lng + 180.0) / 360.0 * n) as u64;
    let lat_rad = lat * PI / 180.0;
    let y = ((1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / PI) / 2.0 * n) as u64;
    format!("https://tile.openstreetmap.org/{zoom}/{x}/{y}.png")
}

// ── ArcGIS FieldMaps deep link ─────────────────────────────────────────────

/// Build a FieldMaps deep link that opens the app and searches for the address.
/// If `item_id` is provided, opens that specific web map.
pub fn build_fieldmaps_url(
    address: &str,
    lat: Option<f64>,
    lng: Option<f64>,
    item_id: Option<&str>,
    scale: u32,
) -> String {
    let mut params = vec![format!("search={}", encode(address))];

    if let Some(id) = item_id {
        params.push(format!("itemID={id}"));
    }
    if let (Some(lat), Some(lng)) = (lat, lng) {
        params.push(format!("center={lat},{lng}"));
        params.push(format!("scale={scale}"));
    }

    format!("https://fieldmaps.arcgis.app?{}", params.join("&"))
}

// ── Waze deep link ─────────────────────────────────────────────────────────

pub fn build_waze_url(lat: f64, lng: f64) -> String {
    format!("https://waze.com/ul?ll={lat},{lng}&navigate=yes&zoom=17")
}

// ── Clipboard / text export ────────────────────────────────────────────────

/// Format the route as plain text suitable for copying to clipboard or SMS.
pub fn format_route_text(addresses: &[String], total_seconds: f64) -> String {
    use crate::solver::format_duration;
    let stops: String = addresses
        .iter()
        .enumerate()
        .map(|(i, a)| format!("  {}. {a}", i + 1))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "OnTrack Route — {} stops · {}\n\n{stops}",
        addresses.len(),
        format_duration(total_seconds)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_url_empty() {
        assert_eq!(
            build_maps_url(&[]),
            "https://www.google.com/maps/dir/?api=1"
        );
    }

    #[test]
    fn maps_url_single() {
        let url = build_maps_url(&["123 Main St".to_string()]);
        assert!(url.contains("destination="));
        assert!(!url.contains(" "));
    }

    #[test]
    fn maps_url_two_stops() {
        let url = build_maps_url(&["A St".to_string(), "B Ave".to_string()]);
        assert!(url.contains("origin="));
        assert!(url.contains("destination="));
        assert!(!url.contains("waypoints="));
    }

    #[test]
    fn maps_url_three_stops_has_waypoint() {
        let url = build_maps_url(&[
            "A St".to_string(),
            "B Ave".to_string(),
            "C Blvd".to_string(),
        ]);
        assert!(url.contains("waypoints="));
    }

    #[test]
    fn waze_url() {
        let url = build_waze_url(47.6588, -117.426);
        // f64 trailing zeros are stripped — match on the significant digits
        assert!(url.contains("47.6588") && url.contains("-117.426"));
        assert!(url.contains("navigate=yes"));
    }

    #[test]
    fn osm_tile_url_no_spaces() {
        let url = build_osm_tile_url(47.6588, -117.4260, 16);
        assert!(url.starts_with("https://tile.openstreetmap.org/16/"));
        assert!(!url.contains(' '));
    }

    #[test]
    fn fieldmaps_url_with_item_id() {
        let url = build_fieldmaps_url("123 Main St", Some(47.0), Some(-117.0), Some("abc123"), 2000);
        assert!(url.contains("itemID=abc123"));
        assert!(url.contains("center="));
    }

    #[test]
    fn export_csv_roundtrip() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let addrs = vec!["123 Main St".to_string(), "456 Elm Ave".to_string()];
        export_csv(&addrs, tmp.path()).unwrap();

        let mut rdr = csv::Reader::from_path(tmp.path()).unwrap();
        let records: Vec<_> = rdr.records().collect::<Result<_, _>>().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].get(0).unwrap(), "1");
        assert_eq!(records[0].get(1).unwrap(), "123 Main St");
    }
}
