// /qompassai/ontrack-rs/crates/ontrack-core/src/lib.rs
// Qompass AI — OnTrack core (Rust)
// Copyright (C) 2026 Qompass AI, All rights reserved.
// ----------------------------------------------------
//! Pure-Rust core for OnTrack route optimization.
//!
//! Modules:
//!   - [`parser`]   — CSV / XLSX → `Vec<String>` addresses
//!   - [`geocoder`] — address → lat/lng via Nominatim or Google
//!   - [`matrix`]   — NxN duration/distance matrix (OSRM, Google, Haversine)
//!   - [`solver`]   — TSP solver (nearest-neighbor + 2-opt local search)
//!   - [`exporter`] — CSV, Google Maps, ArcGIS FieldMaps, Street View, Waze URLs
//!   - [`config`]   — runtime env-var configuration
//!   - [`voice`]    — (feature `voice`) whisper-rs speech to text

pub mod config;
pub mod exporter;
pub mod geocoder;
pub mod matrix;
pub mod parser;
pub mod solver;

#[cfg(feature = "voice")]
pub mod voice;

pub use exporter::{
    build_fieldmaps_url, build_maps_url, build_maps_url_chunked, build_streetview_embed_url,
    build_streetview_url, build_waze_url, export_csv, format_duration,
};
pub use geocoder::{
    geocode_addresses, geocode_address_google, geocode_address_nominatim, get_current_location,
    Location,
};
pub use matrix::{build_distance_matrix, haversine, Backend};
pub use parser::parse_addresses;
pub use solver::{solve_open_tsp, solve_tsp, RouteResult, SolverBackend};
