// /qompassai/ontrack-rs/crates/ontrack-mobile/android/app/build.gradle.kts
// Qompass AI — OnTrack Android app module.
// Copyright (C) 2026 Qompass AI, All rights reserved.
//
// Builds the Android APK / AAB. The native Rust library is compiled
// via the `:cargoBuild` task (defined below) which invokes `cargo`
// for each ABI and copies the resulting `libontrack_mobile.so`
// into `src/main/jniLibs/<abi>/`.
//
// Targets:
//   ./gradlew :app:bundleRelease   →  app/build/outputs/bundle/release/app-release.aab
//   ./gradlew :app:assembleRelease →  app/build/outputs/apk/release/app-release.apk
plugins {
    id("com.android.application")
}

android {
    namespace   = "com.qompassai.ontrack"
    compileSdk  = 35
    ndkVersion  = "27.0.12077973"

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
            val ks = System.getenv("ONTRACK_KEYSTORE") ?: (project.findProperty("ONTRACK_KEYSTORE") as String?)
            if (ks != null) {
                storeFile     = file(ks)
                storePassword = System.getenv("ONTRACK_KEYSTORE_PASS") ?: project.findProperty("ONTRACK_KEYSTORE_PASS") as String?
                keyAlias      = System.getenv("ONTRACK_KEY_ALIAS")     ?: project.findProperty("ONTRACK_KEY_ALIAS") as String?
                keyPassword   = System.getenv("ONTRACK_KEY_PASS")      ?: project.findProperty("ONTRACK_KEY_PASS") as String?
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled     = false
            isShrinkResources   = false
            signingConfig       = signingConfigs.getByName("release")
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
        debug {
            isMinifyEnabled   = false
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

    // Bundle config — single base APK with split ABIs for Play.
    bundle {
        abi { enableSplit = true }
        language { enableSplit = false }
        density { enableSplit = false }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Cargo integration — compiles the Rust workspace's `ontrack-mobile` crate
// for each enabled Android ABI and copies the resulting .so into jniLibs.
// ───────────────────────────────────────────────────────────────────────────
val cargoBuildDir = layout.buildDirectory.dir("cargo")

val rustTargets = mapOf(
    "arm64-v8a"   to "aarch64-linux-android",
    "armeabi-v7a" to "armv7-linux-androideabi"
)

val cargoBuild by tasks.registering {
    group = "build"
    description = "Build the Rust ontrack-mobile cdylib for each Android ABI."
    doLast {
        val workspaceRoot = rootProject.projectDir.parentFile.parentFile.parentFile.parentFile
        // crates/ontrack-mobile/android/ → ../../.. → workspace root

        val profile = if (project.hasProperty("rustDebug")) "debug" else "release"
        val ndk = android.ndkDirectory
        val cargoNdk = System.getenv("ANDROID_NDK_HOME") ?: ndk.absolutePath

        rustTargets.forEach { (abi, target) ->
            exec {
                workingDir = workspaceRoot
                environment("ANDROID_NDK_HOME", cargoNdk)
                environment("CARGO_TARGET_${target.uppercase().replace('-', '_')}_LINKER",
                    "${cargoNdk}/toolchains/llvm/prebuilt/linux-x86_64/bin/${target}26-clang")
                commandLine = buildList {
                    add("cargo")
                    add("ndk")
                    add("--target"); add(target)
                    add("--platform"); add("26")
                    add("--")
                    add("rustc")
                    add("-p"); add("ontrack-mobile")
                    add("--lib")
                    if (profile == "release") add("--release")
                }
            }
            val srcSo = workspaceRoot.resolve("target/$target/$profile/libontrack_mobile.so")
            val dstDir = file("src/main/jniLibs/$abi")
            dstDir.mkdirs()
            copy {
                from(srcSo)
                into(dstDir)
            }
        }
    }
}

tasks.matching { it.name.startsWith("merge") && it.name.endsWith("JniLibFolders") }.configureEach {
    dependsOn(cargoBuild)
}
tasks.matching { it.name.startsWith("package") && (it.name.endsWith("Debug") || it.name.endsWith("Release")) }.configureEach {
    dependsOn(cargoBuild)
}

dependencies {
    // Pure-native app — no Java/Kotlin dependencies required.
}
