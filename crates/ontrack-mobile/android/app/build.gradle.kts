// /qompassai/ontrack-rs/crates/ontrack-mobile/android/app/build.gradle.kts
// Qompass AI — OnTrack Android app module.
// Copyright (C) 2026 Qompass AI, All rights reserved.
//
// This module is consumed *after* the Rust .so files have been cross-compiled
// and staged into `src/main/jniLibs/<abi>/` by `scripts/build-android.sh`.
// Gradle's only job here is to package those .so files plus the manifest into
// an AAB (or APK). All cargo invocation lives in the shell script, which keeps
// this build file Gradle-version-agnostic (no `exec`/`copy` script blocks).
//
// Targets:
//   gradle :app:bundleRelease    →  app/build/outputs/bundle/release/app-release.aab
//   gradle :app:assembleRelease  →  app/build/outputs/apk/release/app-release.apk
plugins {
    id("com.android.application")
}

android {
    namespace   = "com.qompassai.ontrack"
    compileSdk  = 36

    // Auto-detect NDK from ANDROID_NDK_HOME (set by scripts/build-android.sh).
    // We do NOT pin a specific ndkVersion: pinning would fail on machines with
    // a different NDK installed, and our Rust cdylib is already cross-compiled
    // by cargo-ndk before Gradle runs, so AGP's NDK is only used for stripping.
    val ndkHomeEnv: String? = System.getenv("ANDROID_NDK_HOME")
    if (ndkHomeEnv != null) {
        ndkPath = ndkHomeEnv
    }

    defaultConfig {
        applicationId = "com.qompassai.ontrack"
        minSdk        = 26
        targetSdk     = 35
        versionCode   = 200
        versionName   = "2.0.0"

        ndk {
            // Match Cargo build_targets in ontrack-mobile/Cargo.toml.
            abiFilters += listOf("arm64-v8a", "armeabi-v7a")
        }
    }

    signingConfigs {
        create("release") {
            // Override these in ~/.gradle/gradle.properties or env vars:
            //   ONTRACK_KEYSTORE, ONTRACK_KEY_ALIAS, ONTRACK_KEYSTORE_PASS, ONTRACK_KEY_PASS
            val ks = System.getenv("ONTRACK_KEYSTORE")
                ?: (project.findProperty("ONTRACK_KEYSTORE") as String?)
            if (ks != null) {
                storeFile     = file(ks)
                storePassword = System.getenv("ONTRACK_KEYSTORE_PASS")
                    ?: project.findProperty("ONTRACK_KEYSTORE_PASS") as String?
                keyAlias      = System.getenv("ONTRACK_KEY_ALIAS")
                    ?: project.findProperty("ONTRACK_KEY_ALIAS") as String?
                keyPassword   = System.getenv("ONTRACK_KEY_PASS")
                    ?: project.findProperty("ONTRACK_KEY_PASS") as String?
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled     = false
            isShrinkResources   = false
            // Only attach the signing config when the keystore was actually
            // wired up above — otherwise Gradle fails with "Keystore file null
            // not found" on unsigned local builds.
            if (System.getenv("ONTRACK_KEYSTORE") != null
                || project.findProperty("ONTRACK_KEYSTORE") != null) {
                signingConfig = signingConfigs.getByName("release")
            }
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
        debug {
            isMinifyEnabled = false
        }
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // Single base APK with split ABIs for Play.
    bundle {
        abi { enableSplit = true }
        language { enableSplit = false }
        density { enableSplit = false }
    }
}

dependencies {
    // Pure-native app — no Java/Kotlin dependencies required.
}
