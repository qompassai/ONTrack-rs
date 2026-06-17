plugins {
    id("com.android.application")
}

android {
    namespace   = "com.qompassai.ontrack"
    compileSdk  = 36

    val ndkHomeEnv: String? = System.getenv("ANDROID_NDK_HOME")
    if (ndkHomeEnv != null) {
        ndkPath = ndkHomeEnv
    }

    defaultConfig {
        applicationId = "com.qompassai.ontrack"
        minSdk        = 26
        targetSdk     = 35
        versionCode   = 201
        versionName   = "2.0.0"

        ndk {
            abiFilters += listOf("arm64-v8a", "armeabi-v7a")
        }
    }

    signingConfigs {
        create("release") {
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

    bundle {
        abi { enableSplit = true }
        language { enableSplit = false }
        density { enableSplit = false }
    }
}

dependencies {
}
