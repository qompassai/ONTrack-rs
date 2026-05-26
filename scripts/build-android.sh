#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ANDROID_DIR="$ROOT/crates/ontrack-mobile/android"

MODE="${1:-aab}"
case "$MODE" in
    -h|--help)
        awk '/^$/{exit} /^#/{sub(/^# ?/, ""); print}' "$0"
        exit 0
        ;;
    aab|apk|both) ;;
    *)
        echo "✗ unknown mode: $MODE  (use: aab | apk | both | --help)" >&2
        exit 2
        ;;
esac
echo "→ build mode: $MODE"

unset RUSTFLAGS
unset CARGO_BUILD_RUSTFLAGS
unset CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS
unset CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_RUSTFLAGS
export CARGO_ENCODED_RUSTFLAGS=""        # empties any leaking rustflags
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

command -v cargo     >/dev/null  || { echo "✗ cargo not found";    exit 1; }
command -v cargo-ndk >/dev/null  || { echo "✗ cargo-ndk not found (cargo install cargo-ndk)"; exit 1; }
command -v rustup    >/dev/null  || { echo "✗ rustup not found";   exit 1; }

for T in aarch64-linux-android armv7-linux-androideabi; do
    if ! rustup target list --installed | grep -qx "$T"; then
        echo "→ rustup target add $T"
        rustup target add "$T"
    fi
done

: "${ANDROID_HOME:=/opt/android-sdk}"
export ANDROID_HOME
export ANDROID_SDK_ROOT="$ANDROID_HOME"

if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    NDK_DIR=$(ls -d "$ANDROID_HOME"/ndk/* 2>/dev/null | sort -V | tail -n1 || true)
    [ -n "$NDK_DIR" ] || { echo "✗ no NDK installed under $ANDROID_HOME/ndk/"; exit 1; }
    export ANDROID_NDK_HOME="$NDK_DIR"
fi
export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"   # newer cargo-ndk reads this
export NDK_HOME="$ANDROID_NDK_HOME"           # some recipes still use this

if [ -d /usr/lib/jvm/java-17-openjdk ]; then
    export JAVA_HOME=/usr/lib/jvm/java-17-openjdk
    export PATH="$JAVA_HOME/bin:$PATH"
fi

API_LEVEL="${ANDROID_API_LEVEL:-26}"

resolve_android_jar() {
    local newest_jar
    newest_jar=$(ls -1 "$ANDROID_HOME"/platforms/android-*/android.jar 2>/dev/null \
        | sort -V | tail -n1 || true)
    if [ -n "$newest_jar" ]; then
        echo "$newest_jar"
        return 0
    fi
    return 1
}

if ! ANDROID_JAR_PATH=$(resolve_android_jar); then
    echo "⚠ no android.jar under $ANDROID_HOME/platforms/. Attempting install…"
    SDKMGR="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
    if [ ! -x "$SDKMGR" ]; then
        SDKMGR=$(command -v sdkmanager || true)
    fi
    if [ -n "$SDKMGR" ] && [ -x "$SDKMGR" ]; then
        yes | "$SDKMGR" --licenses >/dev/null || true
        "$SDKMGR" "platforms;android-36" "build-tools;36.0.0" "platform-tools"
        ANDROID_JAR_PATH=$(resolve_android_jar) || {
            echo "✗ sdkmanager ran but no platform appeared under $ANDROID_HOME/platforms"
            exit 1
        }
    else
        cat >&2 <<EOF
✗ No Android platform installed and no sdkmanager found.
  Install once via:
    sdkmanager "platforms;android-36" "build-tools;36.0.0" "platform-tools"
  or set ANDROID_JAR=/path/to/android.jar before running this script.
EOF
        exit 1
    fi
fi
export ANDROID_JAR="${ANDROID_JAR:-$ANDROID_JAR_PATH}"
export ANDROID_PLATFORM="${ANDROID_PLATFORM:-$(basename "$(dirname "$ANDROID_JAR")")}"

echo "→ ANDROID_HOME=$ANDROID_HOME"
echo "→ ANDROID_NDK_HOME=$ANDROID_NDK_HOME"
echo "→ ANDROID_JAR=$ANDROID_JAR"
echo "→ ANDROID_PLATFORM=$ANDROID_PLATFORM"
echo "→ JAVA_HOME=${JAVA_HOME:-<system default>}"
echo "→ API level (min): $API_LEVEL"
echo "→ CARGO_TARGET_DIR=$CARGO_TARGET_DIR"
echo

for TRIPLE in aarch64-linux-android armv7-linux-androideabi; do
    echo "→ cargo ndk -t $TRIPLE -p $API_LEVEL build --release -p ontrack-mobile"
    cargo ndk \
        --target "$TRIPLE" \
        --platform "$API_LEVEL" \
        -- build --release -p ontrack-mobile
done

SYMBOLS_DIR="$ANDROID_DIR/app/build/native-debug-symbols"
SYMBOLS_ZIP="$ANDROID_DIR/app/build/outputs/native-debug-symbols/native-debug-symbols.zip"
rm -rf "$SYMBOLS_DIR"
mkdir -p "$SYMBOLS_DIR" "$(dirname "$SYMBOLS_ZIP")"

NDK_STRIP=$(find "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt" \
    -maxdepth 3 -name 'llvm-strip' -type f 2>/dev/null | head -n1 || true)
[ -x "$NDK_STRIP" ] || { echo "✗ llvm-strip not found under $ANDROID_NDK_HOME"; exit 1; }
echo "→ using strip: $NDK_STRIP"

for PAIR in "arm64-v8a:aarch64-linux-android" "armeabi-v7a:armv7-linux-androideabi"; do
    ABI=${PAIR%%:*}; TRIPLE=${PAIR##*:}
    SRC="$CARGO_TARGET_DIR/$TRIPLE/release/libontrack_mobile.so"
    DST="$ANDROID_DIR/app/src/main/jniLibs/$ABI"
    SYM_ABI="$SYMBOLS_DIR/$ABI"
    [ -f "$SRC" ] || { echo "✗ missing $SRC"; exit 1; }
    mkdir -p "$DST" "$SYM_ABI"

    cp "$SRC" "$SYM_ABI/libontrack_mobile.so"
    "$NDK_STRIP" --strip-unneeded "$SRC" -o "$DST/libontrack_mobile.so"
    echo "  $ABI:"
    echo "    stripped (ship): $(stat -c%s "$DST/libontrack_mobile.so") bytes → $DST/libontrack_mobile.so"
    echo "    debug   (zip):   $(stat -c%s "$SYM_ABI/libontrack_mobile.so") bytes → $SYM_ABI/libontrack_mobile.so"
done

rm -f "$SYMBOLS_ZIP"
(cd "$SYMBOLS_DIR" && zip -qr "$SYMBOLS_ZIP" .)
echo "→ native debug symbols: $SYMBOLS_ZIP ($(stat -c%s "$SYMBOLS_ZIP") bytes)"

cd "$ANDROID_DIR"

if [ -x ./gradlew ]; then
    GRADLE=./gradlew
elif command -v gradle >/dev/null; then
    GRADLE=gradle
else
    echo "✗ neither ./gradlew nor system gradle is available"
    exit 1
fi

GRADLE_TASKS=()
case "$MODE" in
    aab)  GRADLE_TASKS+=(:app:bundleRelease) ;;
    apk)  GRADLE_TASKS+=(:app:assembleRelease) ;;
    both) GRADLE_TASKS+=(:app:bundleRelease :app:assembleRelease) ;;
esac

echo "→ gradle tasks: ${GRADLE_TASKS[*]}"
"$GRADLE" "${GRADLE_TASKS[@]}"

echo
if [[ "$MODE" == "aab" || "$MODE" == "both" ]]; then
    echo "→ AAB ready (upload to Play Console):"
    ls -la "$ANDROID_DIR/app/build/outputs/bundle/release/"
    echo
    echo "→ native debug symbols ready (upload alongside the AAB):"
    ls -la "$ANDROID_DIR/app/build/outputs/native-debug-symbols/"
fi
if [[ "$MODE" == "apk" || "$MODE" == "both" ]]; then
    echo
    echo "→ APK ready (sideload-friendly; send directly to testers):"
    ls -la "$ANDROID_DIR/app/build/outputs/apk/release/"
    cat <<'EOF'

  Tester install steps:
    1. Send app-release.apk to the tester (Signal/email/cloud).
    2. On their phone: Settings -> Apps -> Special app access ->
       Install unknown apps -> allow the source app (e.g. Files).
    3. Tap the .apk in their file manager. Play Protect may warn;
       tap 'Install anyway'.
EOF
fi
