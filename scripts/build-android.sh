#!/usr/bin/env bash
# scripts/build-android.sh — Build a signed AAB for Google Play Console.
#
# Requires:
#   - cargo-ndk            (cargo install cargo-ndk)
#   - rustup targets       (aarch64-linux-android, armv7-linux-androideabi)
#   - Android SDK + NDK    (/opt/android-sdk by default on Arch)
#   - JDK 17               (jdk17-openjdk)
#   - Upload keystore + ONTRACK_KEYSTORE/_PASS/_ALIAS/_KEY_PASS in
#     ~/.gradle/gradle.properties or the environment.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ANDROID_DIR="$ROOT/crates/ontrack-mobile/android"

# Pre-flight checks
command -v cargo >/dev/null      || { echo "cargo not found";    exit 1; }
command -v cargo-ndk >/dev/null  || { echo "cargo-ndk not found (cargo install cargo-ndk)"; exit 1; }

: "${ANDROID_HOME:=/opt/android-sdk}"
export ANDROID_HOME
if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    NDK_DIR=$(ls -d "$ANDROID_HOME"/ndk/* 2>/dev/null | sort -V | tail -n1 || true)
    [ -n "$NDK_DIR" ] || { echo "no NDK installed under $ANDROID_HOME/ndk/"; exit 1; }
    export ANDROID_NDK_HOME="$NDK_DIR"
fi
echo "→ ANDROID_HOME=$ANDROID_HOME"
echo "→ ANDROID_NDK_HOME=$ANDROID_NDK_HOME"

# Build native libs for both ABIs
for TRIPLE in aarch64-linux-android armv7-linux-androideabi; do
    echo "→ cargo ndk → $TRIPLE"
    cargo ndk --target "$TRIPLE" --platform 26 -- rustc -p ontrack-mobile --lib --release
done

# Copy .so into Gradle jniLibs
for PAIR in "arm64-v8a:aarch64-linux-android" "armeabi-v7a:armv7-linux-androideabi"; do
    ABI=${PAIR%%:*}; TRIPLE=${PAIR##*:}
    SRC="$ROOT/target/$TRIPLE/release/libontrack_mobile.so"
    DST="$ANDROID_DIR/app/src/main/jniLibs/$ABI"
    mkdir -p "$DST"
    cp -v "$SRC" "$DST/"
done

# Gradle assemble release AAB
cd "$ANDROID_DIR"
./gradlew :app:bundleRelease

echo
echo "→ AAB ready:"
ls -la "$ANDROID_DIR/app/build/outputs/bundle/release/"
