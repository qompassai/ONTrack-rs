use ontrack_core::{
    build_fieldmaps_url, build_maps_url, build_maps_url_chunked, build_waze_url, export_csv,
};
use std::fs;

#[test]
fn maps_url_chunks_over_ten_stops() {
    let stops: Vec<String> = (0..23).map(|i| format!("Stop {i}")).collect();
    let urls = build_maps_url_chunked(&stops);
    assert_eq!(urls.len(), 3);
}

#[test]
fn fieldmaps_url_includes_item_id_when_present() {
    let url = build_fieldmaps_url("123 Main", Some(47.6), Some(-117.4), Some("abc123"), 2000);
    assert!(url.contains("itemID=abc123"));
    assert!(url.contains("center=47.6,-117.4"));
}

#[test]
fn waze_url_format() {
    let url = build_waze_url(47.6, -117.4);
    assert_eq!(url, "https://waze.com/ul?ll=47.6,-117.4&navigate=yes&zoom=17");
}

#[test]
fn csv_export_roundtrip() {
    let stops = vec!["123 Main".to_string(), "456 Elm".to_string()];
    let path = std::env::temp_dir().join("ontrack-rs-test.csv");
    export_csv(&stops, &path).unwrap();
    let body = fs::read_to_string(&path).unwrap();
    assert!(body.starts_with("stop,address"));
    assert!(body.contains("1,\"123 Main\""));
    assert!(body.contains("2,\"456 Elm\""));
    fs::remove_file(&path).ok();
}

#[test]
fn maps_url_single_stop_smoke() {
    let url = build_maps_url(&vec!["123 Main".to_string()]);
    assert!(url.contains("destination=123"));
}
