# Changelog

## 2.0.0 — 2026-05-25
- Initial pure-Rust port of `qompassai/ontrack`.
- Workspace crates: `ontrack-core`, `ontrack-desktop`, `ontrack-mobile`.
- Desktop GUI: `egui`/`eframe`.
- Mobile GUI: `slint` with Android target via `cargo-ndk` + Gradle (AAB).
- Solver: nearest-neighbour + 2-opt local search (replaces OR-Tools).
- Voice input (feature-gated): `cpal` + `whisper-rs` (whisper.cpp).
- Distance backends: OSRM, Google, Haversine.
