#!/usr/bin/env bash
# geny.sh
# ----------------------------------------
set -euo pipefail
ROOT="$(pwd)"
APP_ID="${APP_ID:-com.qompassai.ontrack}"
APK_PATH="${APK_PATH:-$ROOT/ontrack-mobile/android/app/build/outputs/apk/release/app-release.apk}"
SCREENSHOT_DIR="${SCREENSHOT_DIR:-$ROOT/screenshots}"
ADB_BIN="${ADB_BIN:-adb}"
usage()
{
    cat << EOM
ONTrack Genymotion/ADB workflow
Usage:
  $(basename "$0") devices
  $(basename "$0") install
  $(basename "$0") launch
  $(basename "$0") screenshot [name]
  $(basename "$0") screenshot-seq [base] [count]
  $(basename "$0") logcat
  $(basename "$0") full [basename] [count]
Before use:
  1. Start a Genymotion device manually.
  2. Ensure it appears in: adb devices
EOM
}
adb_wait()
{
    "$ADB_BIN" wait-for-device
    until "$ADB_BIN" shell getprop sys.boot_completed 2> /dev/null | tr -d '\r' | grep -qx '1'; do
        sleep 2
    done
}
case "${1:-}" in
    devices)
        "$ADB_BIN" devices
        ;;
    install)
        [ -f "$APK_PATH" ] || {
            echo "✗ APK not found: $APK_PATH"
            exit 1
        }
        adb_wait
        "$ADB_BIN" install -r "$APK_PATH"
        ;;
    launch)
        adb_wait
        "$ADB_BIN" shell monkey -p "$APP_ID" -c android.intent.category.LAUNCHER 1 > /dev/null 2>&1
        ;;
    screenshot)
        mkdir -p "$SCREENSHOT_DIR"
        NAME="${2:-$(date +%Y%m%d-%H%M%S)}"
        adb_wait
        "$ADB_BIN" exec-out screencap -p > "$SCREENSHOT_DIR/${NAME}.png"
        echo "→ saved $SCREENSHOT_DIR/${NAME}.png"
        ;;
    screenshot-seq)
        mkdir -p "$SCREENSHOT_DIR"
        BASE="${2:-ontrack}"
        COUNT="${3:-3}"
        adb_wait
        i=1
        while [ "$i" -le "$COUNT" ]; do
            "$ADB_BIN" exec-out screencap -p > "$SCREENSHOT_DIR/${BASE}-${i}.png"
            echo "→ saved $SCREENSHOT_DIR/${BASE}-${i}.png"
            i=$((i + 1))
            [ "$i" -le "$COUNT" ] && {
                echo "→ prepare next screen, then press Enter"
                read -r _
            }
        done
        ;;
    logcat)
        adb_wait
        exec "$ADB_BIN" logcat
        ;;
    full)
        BASE="${2:-ontrack}"
        COUNT="${3:-3}"
        "$0" install
        "$0" launch
        echo "→ app launched; navigate to first screen, then press Enter"
        read -r _
        "$0" screenshot-seq "$BASE" "$COUNT"
        ;;
    -h | --help | "")
        usage
        ;;
    *)
        echo "✗ unknown command: $1"
        usage
        exit 2
        ;;
esac
