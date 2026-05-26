// /qompassai/ontrack-rs/crates/ontrack-mobile/android/build.gradle.kts
// Qompass AI — OnTrack root Gradle build
// Copyright (C) 2026 Qompass AI, All rights reserved.
plugins {
    id("com.android.application") version "8.3.2" apply false
}

tasks.register<Delete>("clean") {
    delete(rootProject.buildDir)
}
