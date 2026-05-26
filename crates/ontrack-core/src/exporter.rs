
use anyhow::{anyhow, Result};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use urlencoding::encode;


pub fn export_csv<P: AsRef<Path>>(ordered_addresses: &[String], output_path: P) -> Result<()> {
    let mut f = File::create(output_path)?;
    writeln!(f, "stop,address")?;
    for (i, addr) in ordered_addresses.iter().enumerate() {
        let escaped = addr.replace('"', "\"\"");
        writeln!(f, "{},\"{}\"", i + 1, escaped)?;
    }
    Ok(())
}


pub fn build_maps_url(ordered_addresses: &[String]) -> String {
    if ordered_addresses.is_empty() {
        return "https://www.google.com/maps/dir/?api=1".to_string();
    }
    if ordered_addresses.len() == 1 {
        let enc = encode(&ordered_addresses[0]);
        return format!(
            "https://www.google.com/maps/dir/?api=1&destination={enc}&travelmode=driving"
        );
    }
    let origin = encode(&ordered_addresses[0]);
    let destination = encode(&ordered_addresses[ordered_addresses.len() - 1]);
    let mut base = format!(
        "https://www.google.com/maps/dir/?api=1&origin={origin}&destination={destination}&travelmode=driving"
    );
    let waypoints = &ordered_addresses[1..ordered_addresses.len() - 1];
    if !waypoints.is_empty() {
        let joined = waypoints.join("|");
        let enc = encode(&joined);
        base.push_str(&format!("&waypoints={enc}"));
    }
    base
}

pub fn build_maps_url_chunked(ordered_addresses: &[String]) -> Vec<String> {
    ordered_addresses
        .chunks(10)
        .map(|chunk| build_maps_url(&chunk.to_vec()))
        .collect()
}


#[allow(clippy::too_many_arguments)]
pub fn build_streetview_url(
    lat: Option<f64>,
    lng: Option<f64>,
    address: Option<&str>,
    api_key: &str,
    width: u32,
    height: u32,
    heading: Option<i32>,
    pitch: i32,
    fov: i32,
) -> Result<String> {
    let mut params: Vec<(String, String)> = Vec::new();
    params.push(("size".into(), format!("{width}x{height}")));
    params.push(("pitch".into(), pitch.to_string()));
    params.push(("fov".into(), fov.to_string()));
    params.push(("key".into(), api_key.to_string()));

    let location = if let (Some(la), Some(ln)) = (lat, lng) {
        format!("{la},{ln}")
    } else if let Some(a) = address {
        a.to_string()
    } else {
        return Err(anyhow!("Either lat/lng or address must be provided."));
    };
    params.push(("location".into(), location));
    if let Some(h) = heading {
        params.push(("heading".into(), h.to_string()));
    }

    let qs: Vec<String> = params
        .into_iter()
        .map(|(k, v)| format!("{}={}", encode(&k), encode(&v)))
        .collect();
    Ok(format!(
        "https://maps.googleapis.com/maps/api/streetview?{}",
        qs.join("&")
    ))
}

pub fn build_streetview_embed_url(lat: f64, lng: f64) -> String {
    format!("https://www.google.com/maps/@{lat},{lng},3a,90y,0h,90t/data=!3m4!1e1!3m2!1s!2e0")
}


pub fn build_fieldmaps_url(
    address: &str,
    lat: Option<f64>,
    lng: Option<f64>,
    item_id: Option<&str>,
    scale: u32,
) -> String {
    let mut parts = Vec::new();
    parts.push(format!("search={}", encode(address)));
    if let Some(id) = item_id.filter(|s| !s.is_empty()) {
        parts.push(format!("itemID={}", encode(id)));
    }
    if let (Some(la), Some(ln)) = (lat, lng) {
        parts.push(format!("center={la},{ln}"));
        parts.push(format!("scale={scale}"));
    }
    format!("https://fieldmaps.arcgis.app?{}", parts.join("&"))
}


pub fn build_waze_url(lat: f64, lng: f64) -> String {
    format!("https://waze.com/ul?ll={lat},{lng}&navigate=yes&zoom=17")
}


pub fn format_duration(seconds: f64) -> String {
    let mins = (seconds / 60.0) as u64;
    if mins < 60 {
        format!("{mins} min")
    } else {
        let h = mins / 60;
        let m = mins % 60;
        if m == 0 {
            format!("{h}h")
        } else {
            format!("{h}h {m}m")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_destination_url() {
        let url = build_maps_url(&vec!["123 Main St".to_string()]);
        assert!(url.contains("destination=123%20Main%20St") || url.contains("destination=123+Main+St") || url.contains("destination=123%20Main"));
        assert!(url.contains("travelmode=driving"));
    }

    #[test]
    fn duration_format() {
        assert_eq!(format_duration(45.0), "0 min");
        assert_eq!(format_duration(60.0 * 30.0), "30 min");
        assert_eq!(format_duration(60.0 * 90.0), "1h 30m");
        assert_eq!(format_duration(60.0 * 120.0), "2h");
    }
}
