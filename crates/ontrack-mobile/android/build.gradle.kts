// /qompassai/ontrack-rs/crates/ontrack-mobile/android/build.gradle.kts
// Qompass AI — OnTrack root Gradle build
// Copyright (C) 2026 Qompass AI, All rights reserved.
plugins {
    // AGP 9.2.0 (April 2026) requires Gradle 9.4.1+. Matches Gradle 9.5.1 on Arch.
    // Pin here so the plugin version doesn't drift across machines.
    id("com.android.application") version "9.2.0" apply false
}

// `rootProject.buildDir` is deprecated in Gradle 8+ and removed in 9.x; use
// the `layout.buildDirectory` Provider API instead.
tasks.register<Delete>("clean") {
    delete(rootProject.layout.buildDirectory)
}
