// /qompassai/ontrack-rs/crates/ontrack-mobile/build.rs
// Qompass AI — OnTrack mobile: compile .slint UI at build time.
// Copyright (C) 2026 Qompass AI, All rights reserved.
fn main() {
    slint_build::compile("ui/app.slint").expect("slint compile");
}
