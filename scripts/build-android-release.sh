#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ANDROID_DIR="$ROOT/crates/ontrack-mobile/android"
MODE="${1:-aab}"
CLEAN_BUILD="${CLEAN_BUILD:-1}"
STRIP_NATIVE="${STRIP_NATIVE:-1}"

case "$MODE" in
    -h | --help)
        cat << 'EOM'
Build Android release artifacts for ONTrack.

Usage:
  ./build-android-build-release.sh [aab|apk|both]

Environment:
  CLEAN_BUILD=1   Remove prior Rust/Gradle outputs before building (default: 1)
  STRIP_NATIVE=1  Strip native libraries and create native debug symbols zip (default: 1)
  ANDROID_HOME    Android SDK root (default: /opt/android-sdk)
  ANDROID_NDK_HOME Android NDK root (auto-detected if unset)
  ANDROID_API_LEVEL Android min API level passed to cargo-ndk (default: 26)
EOM
        exit 0
        ;;
    aab | apk | both) ;;
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
export CARGO_ENCODED_RUSTFLAGS=""
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

for cmd in cargo cargo-ndk rustup python3; do
    command -v "$cmd" > /dev/null || {
        echo "✗ $cmd not found"
        exit 1
    }
done

for T in aarch64-linux-android armv7-linux-androideabi; do
    if ! rustup target list --installed | grep -qx "$T"; then
        echo "→ rustup target add $T"
        rustup target add "$T"
    fi
done

: "${ANDROID_HOME:=/opt/android-sdk}"
export ANDROID_HOME
export ANDROID_SDK_ROOT="$ANDROID_HOME"

find_ndk_root()
{
    local candidate=""

    if [ -n "${ANDROID_NDK_HOME:-}" ] && [ -d "$ANDROID_NDK_HOME" ]; then
        candidate="$ANDROID_NDK_HOME"
        if find "$candidate" -type f -name llvm-strip 2> /dev/null | grep -q .; then
            printf '%s\n' "$candidate"
            return 0
        fi
    fi

    if [ -d "$ANDROID_HOME/ndk" ]; then
        candidate=$(find "$ANDROID_HOME/ndk" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -n1 || true)
        if [ -n "$candidate" ] && find "$candidate" -type f -name llvm-strip 2> /dev/null | grep -q .; then
            printf '%s\n' "$candidate"
            return 0
        fi
    fi

    if [ -d /opt/android-ndk ] && find /opt/android-ndk -type f -name llvm-strip 2> /dev/null | grep -q .; then
        printf '%s\n' "/opt/android-ndk"
        return 0
    fi

    if [ -d /opt/android-ndk ]; then
        printf '%s\n' "/opt/android-ndk"
        return 0
    fi

    return 1
}

ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$(find_ndk_root || true)}"
[ -n "$ANDROID_NDK_HOME" ] || {
    echo "✗ could not determine ANDROID_NDK_HOME"
    exit 1
}
export ANDROID_NDK_HOME
export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"
export NDK_HOME="$ANDROID_NDK_HOME"

if [ -d /usr/lib/jvm/java-17-openjdk ]; then
    export JAVA_HOME=/usr/lib/jvm/java-17-openjdk
    export PATH="$JAVA_HOME/bin:$PATH"
fi

API_LEVEL="${ANDROID_API_LEVEL:-26}"

resolve_android_jar()
{
    local newest_jar
    newest_jar=$(find "$ANDROID_HOME/platforms" -mindepth 2 -maxdepth 2 -type f -name android.jar 2> /dev/null | sort -V | tail -n1 || true)
    if [ -n "$newest_jar" ]; then
        printf '%s\n' "$newest_jar"
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
        yes | "$SDKMGR" --licenses > /dev/null || true
        "$SDKMGR" "platforms;android-36" "build-tools;36.0.0" "platform-tools"
        ANDROID_JAR_PATH=$(resolve_android_jar) || {
            echo "✗ sdkmanager ran but no platform appeared under $ANDROID_HOME/platforms"
            exit 1
        }
    else
        cat >&2 << 'EOM'
✗ No Android platform installed and no sdkmanager found.
  Install once via:
    sdkmanager "platforms;android-36" "build-tools;36.0.0" "platform-tools"
  or set ANDROID_JAR=/path/to/android.jar before running this script.
EOM
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

[ -d "$ANDROID_DIR" ] || {
    echo "✗ Android project dir not found: $ANDROID_DIR"
    exit 1
}

if [ "$CLEAN_BUILD" = "1" ]; then
    echo "→ cleaning Rust target dir"
    cargo clean --target-dir "$CARGO_TARGET_DIR" || true
    echo "→ cleaning Android build dir"
    rm -rf "$ANDROID_DIR/app/build"
    if [ -x "$ANDROID_DIR/gradlew" ]; then
        (cd "$ANDROID_DIR" && ./gradlew clean)
    fi
    echo
fi

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

find_llvm_strip()
{
    local root="$1"
    local direct="$root/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip"
    if [ -x "$direct" ]; then
        printf '%s\n' "$direct"
        return 0
    fi
    find "$root/toolchains/llvm/prebuilt" -type f -name llvm-strip 2> /dev/null | sort | head -n1 || true
}

NDK_STRIP="$(find_llvm_strip "$ANDROID_NDK_HOME")"
if [ "$STRIP_NATIVE" = "1" ] && [ -x "$NDK_STRIP" ]; then
    echo "→ using strip: $NDK_STRIP"
    for PAIR in "arm64-v8a:aarch64-linux-android" "armeabi-v7a:armv7-linux-androideabi"; do
        ABI=${PAIR%%:*}
        TRIPLE=${PAIR##*:}
        SRC="$CARGO_TARGET_DIR/$TRIPLE/release/libontrack_mobile.so"
        DST="$ANDROID_DIR/app/src/main/jniLibs/$ABI"
        SYM_ABI="$SYMBOLS_DIR/$ABI"
        [ -f "$SRC" ] || {
            echo "✗ missing $SRC"
            exit 1
        }
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
else
    echo "⚠ llvm-strip not found under $ANDROID_NDK_HOME; continuing without native strip step"
    for PAIR in "arm64-v8a:aarch64-linux-android" "armeabi-v7a:armv7-linux-androideabi"; do
        ABI=${PAIR%%:*}
        TRIPLE=${PAIR##*:}
        SRC="$CARGO_TARGET_DIR/$TRIPLE/release/libontrack_mobile.so"
        DST="$ANDROID_DIR/app/src/main/jniLibs/$ABI"
        [ -f "$SRC" ] || {
            echo "✗ missing $SRC"
            exit 1
        }
        mkdir -p "$DST"
        cp "$SRC" "$DST/libontrack_mobile.so"
        echo "  $ABI: copied unstripped library → $DST/libontrack_mobile.so"
    done
fi

echo
cd "$ANDROID_DIR"
if [ -x ./gradlew ]; then
    GRADLE=./gradlew
elif command -v gradle > /dev/null; then
    GRADLE=gradle
else
    echo "✗ neither ./gradlew nor system gradle is available"
    exit 1
fi

GRADLE_TASKS=()
case "$MODE" in
    aab) GRADLE_TASKS+=(clean :app:bundleRelease) ;;
    apk) GRADLE_TASKS+=(clean :app:assembleRelease) ;;
    both) GRADLE_TASKS+=(clean :app:bundleRelease :app:assembleRelease) ;;
esac

echo "→ gradle tasks: ${GRADLE_TASKS[*]}"
"$GRADLE" "${GRADLE_TASKS[@]}"
echo

abs_path()
{
    local p="$1"
    [ -e "$p" ] || return 1
    python3 - << PY
from pathlib import Path
print(Path(r'''$p''').resolve())
PY
}

LATEST_AAB=$(find "$ANDROID_DIR/app/build/outputs/bundle/release" -maxdepth 1 -type f -name '*.aab' 2> /dev/null | sort | tail -n1 || true)
LATEST_APK=$(find "$ANDROID_DIR/app/build/outputs/apk/release" -maxdepth 1 -type f -name '*.apk' 2> /dev/null | sort | tail -n1 || true)
LATEST_SYMBOLS=$(find "$ANDROID_DIR/app/build/outputs/native-debug-symbols" -maxdepth 2 -type f -name '*.zip' 2> /dev/null | sort | tail -n1 || true)

echo "== upload artifacts =="
if [ -n "$LATEST_AAB" ]; then
    echo "AAB=$LATEST_AAB"
    echo "AAB_ABS=$(abs_path "$LATEST_AAB")"
fi
if [ -n "$LATEST_SYMBOLS" ]; then
    echo "NATIVE_SYMBOLS_ZIP=$LATEST_SYMBOLS"
    echo "NATIVE_SYMBOLS_ZIP_ABS=$(abs_path "$LATEST_SYMBOLS")"
fi
if [ -n "$LATEST_APK" ]; then
    echo "APK=$LATEST_APK"
    echo "APK_ABS=$(abs_path "$LATEST_APK")"
fi
echo

if [[ $MODE == "aab" || $MODE == "both" ]]; then
    echo "→ AAB ready (upload to Play Console):"
    ls -la "$ANDROID_DIR/app/build/outputs/bundle/release/"
    echo
    if [ -d "$ANDROID_DIR/app/build/outputs/native-debug-symbols" ]; then
        echo "→ native debug symbols ready (upload alongside the AAB):"
        ls -la "$ANDROID_DIR/app/build/outputs/native-debug-symbols/"
    fi
fi
if [[ $MODE == "apk" || $MODE == "both" ]]; then
    echo
    echo "→ APK ready (sideload-friendly; send directly to testers):"
    ls -la "$ANDROID_DIR/app/build/outputs/apk/release/"
    cat << 'EOM'

  Tester install steps:
    1. Send app-release.apk to the tester (Signal/email/cloud).
    2. On their phone: Settings -> Apps -> Special app access ->
       Install unknown apps -> allow the source app (e.g. Files).
    3. Tap the .apk in their file manager. Play Protect may warn;
       tap 'Install anyway'.
EOM
fi
