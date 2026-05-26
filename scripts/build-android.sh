#!/usr/bin/env bash
# scripts/build-android.sh — Build a signed AAB for Google Play Console.
#
# Requires on the host:
#   - rustup, with cross targets:
#       rustup target add aarch64-linux-android armv7-linux-androideabi
#   - cargo-ndk:
#       cargo install cargo-ndk
#   - Android SDK + NDK 25b or newer (NDK r26+ recommended for slint 1.9)
#       env: ANDROID_HOME (default /opt/android-sdk on Arch)
#       env: ANDROID_NDK_HOME (auto-detected from $ANDROID_HOME/ndk/* if unset)
#   - JDK 17 (jdk17-openjdk on Arch)
#   - Upload keystore + ONTRACK_KEYSTORE/_PASS/_ALIAS/_KEY_PASS exported, or
#     keys configured in ~/.gradle/gradle.properties.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ANDROID_DIR="$ROOT/crates/ontrack-mobile/android"

# ── Sanitize host-only rustflags ─────────────────────────────────────────────
# A common Arch/Hyprland setup exports `-C target-cpu=native` (or sets the
# equivalent in ~/.cargo/config.toml under [build] rustflags). When cargo-ndk
# invokes the NDK's aarch64 clang with those flags, clang prints things like
#   '+vaes' is not a recognized feature for this target
#   'raptorlake' is not a recognized processor for this target
# and ultimately fails to link. Strip all such env vars *and* tell cargo to
# ignore any rustflags from the user's global cargo config for this build.
unset RUSTFLAGS
unset CARGO_BUILD_RUSTFLAGS
unset CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS
unset CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_RUSTFLAGS
export CARGO_ENCODED_RUSTFLAGS=""        # empties any leaking rustflags
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

# ── Tooling pre-flight ───────────────────────────────────────────────────────
command -v cargo     >/dev/null  || { echo "✗ cargo not found";    exit 1; }
command -v cargo-ndk >/dev/null  || { echo "✗ cargo-ndk not found (cargo install cargo-ndk)"; exit 1; }
command -v rustup    >/dev/null  || { echo "✗ rustup not found";   exit 1; }

# Ensure both Android targets are installed (idempotent).
for T in aarch64-linux-android armv7-linux-androideabi; do
    if ! rustup target list --installed | grep -qx "$T"; then
        echo "→ rustup target add $T"
        rustup target add "$T"
    fi
done

# ── Android SDK / NDK ────────────────────────────────────────────────────────
: "${ANDROID_HOME:=/opt/android-sdk}"
export ANDROID_HOME
export ANDROID_SDK_ROOT="$ANDROID_HOME"

if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    # Pick the newest NDK installed; cargo-ndk wants an absolute path.
    NDK_DIR=$(ls -d "$ANDROID_HOME"/ndk/* 2>/dev/null | sort -V | tail -n1 || true)
    [ -n "$NDK_DIR" ] || { echo "✗ no NDK installed under $ANDROID_HOME/ndk/"; exit 1; }
    export ANDROID_NDK_HOME="$NDK_DIR"
fi
export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"   # newer cargo-ndk reads this
export NDK_HOME="$ANDROID_NDK_HOME"           # some recipes still use this

# Pin JDK 17 if available — newer JDKs break the Android Gradle Plugin.
if [ -d /usr/lib/jvm/java-17-openjdk ]; then
    export JAVA_HOME=/usr/lib/jvm/java-17-openjdk
    export PATH="$JAVA_HOME/bin:$PATH"
fi

API_LEVEL="${ANDROID_API_LEVEL:-26}"

echo "→ ANDROID_HOME=$ANDROID_HOME"
echo "→ ANDROID_NDK_HOME=$ANDROID_NDK_HOME"
echo "→ JAVA_HOME=${JAVA_HOME:-<system default>}"
echo "→ API level: $API_LEVEL"
echo "→ CARGO_TARGET_DIR=$CARGO_TARGET_DIR"
echo

# ── Cross-compile native libs ────────────────────────────────────────────────
# `cargo ndk` injects the right CC/CXX/AR/linker for the NDK toolchain.
# We invoke `cargo build` (not `cargo rustc --lib`) so the cdylib target is
# picked up automatically per the [lib] crate-type = ["cdylib", "rlib"] in
# the mobile crate's Cargo.toml.
for TRIPLE in aarch64-linux-android armv7-linux-androideabi; do
    echo "→ cargo ndk -t $TRIPLE -p $API_LEVEL build --release -p ontrack-mobile"
    cargo ndk \
        --target "$TRIPLE" \
        --platform "$API_LEVEL" \
        -- build --release -p ontrack-mobile
done

# ── Stage .so files into Gradle jniLibs ──────────────────────────────────────
for PAIR in "arm64-v8a:aarch64-linux-android" "armeabi-v7a:armv7-linux-androideabi"; do
    ABI=${PAIR%%:*}; TRIPLE=${PAIR##*:}
    SRC="$CARGO_TARGET_DIR/$TRIPLE/release/libontrack_mobile.so"
    DST="$ANDROID_DIR/app/src/main/jniLibs/$ABI"
    [ -f "$SRC" ] || { echo "✗ missing $SRC"; exit 1; }
    mkdir -p "$DST"
    cp -v "$SRC" "$DST/"
done

# ── Gradle: assemble release AAB ─────────────────────────────────────────────
cd "$ANDROID_DIR"

# If a wrapper is checked in, prefer it; otherwise fall back to system gradle.
if [ -x ./gradlew ]; then
    GRADLE=./gradlew
elif command -v gradle >/dev/null; then
    GRADLE=gradle
else
    echo "✗ neither ./gradlew nor system gradle is available"
    exit 1
fi

"$GRADLE" :app:bundleRelease

echo
echo "→ AAB ready:"
ls -la "$ANDROID_DIR/app/build/outputs/bundle/release/"
