#!/usr/bin/env bash
set -euo pipefail

ROOT="$(pwd)"
APP_ID="${APP_ID:-com.qompassai.ontrack}"
DEFAULT_APK_PATH_1="$ROOT/crates/ontrack-mobile/android/app/build/outputs/apk/release/app-release.apk"
DEFAULT_APK_PATH_2="$ROOT/ontrack-mobile/android/app/build/outputs/apk/release/app-release.apk"
APK_PATH="${APK_PATH:-}"
if [ -z "$APK_PATH" ]; then
    if [ -f "$DEFAULT_APK_PATH_1" ]; then
        APK_PATH="$DEFAULT_APK_PATH_1"
    else
        APK_PATH="$DEFAULT_APK_PATH_2"
    fi
fi
SCREENSHOT_DIR="${SCREENSHOT_DIR:-$ROOT/screenshots}"
AVD_NAME="${AVD_NAME:-ontrack-pixel}"
ANDROID_HOME="${ANDROID_HOME:-/opt/android-sdk}"
ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-$ANDROID_HOME}"
SDKMANAGER="${SDKMANAGER:-$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager}"
AVDMANAGER="${AVDMANAGER:-$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager}"
EMULATOR_BIN="${EMULATOR_BIN:-$ANDROID_HOME/emulator/emulator}"
ADB_BIN="${ADB_BIN:-$ANDROID_HOME/platform-tools/adb}"
SYSTEM_IMAGE="${SYSTEM_IMAGE:-system-images;android-36;google_apis;x86_64}"
DEVICE_PROFILE="${DEVICE_PROFILE:-pixel}"

export ANDROID_HOME ANDROID_SDK_ROOT
if [ -z "${ANDROID_AVD_HOME:-}" ] && [ -d "$HOME/.config/.android/avd" ]; then
    export ANDROID_AVD_HOME="$HOME/.config/.android/avd"
fi
export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$ANDROID_HOME/cmdline-tools/latest/bin:$PATH"

usage()
{
    cat << EOM
ONTrack Android CLI workflow

Usage:
  $(basename "$0") setup-sdk
  $(basename "$0") create-avd
  $(basename "$0") list-avds
  $(basename "$0") start-emulator
  $(basename "$0") wait-device
  $(basename "$0") install
  $(basename "$0") launch
  $(basename "$0") screenshot [name]
  $(basename "$0") screenshot-seq [base] [count]
  $(basename "$0") logcat
  $(basename "$0") doctor
  $(basename "$0") full [basename] [count]

Environment overrides:
  APP_ID, APK_PATH, SCREENSHOT_DIR, AVD_NAME, ANDROID_HOME,
  SYSTEM_IMAGE, DEVICE_PROFILE, ADB_BIN, EMULATOR_BIN
EOM
}

need()
{
    command -v "$1" > /dev/null 2>&1 || {
        echo "✗ missing command: $1"
        exit 1
    }
}

adb_wait()
{
    "$ADB_BIN" wait-for-device
    until "$ADB_BIN" shell getprop sys.boot_completed 2> /dev/null | tr -d '\r' | grep -qx '1'; do
        sleep 2
    done
}

case "${1:-}" in
    setup-sdk)
        need yes
        [ -x "$SDKMANAGER" ] || {
            echo "✗ sdkmanager not found at $SDKMANAGER"
            exit 1
        }
        yes | "$SDKMANAGER" --licenses > /dev/null || true
        "$SDKMANAGER" \
            "platform-tools" \
            "emulator" \
            "platforms;android-36" \
            "$SYSTEM_IMAGE"
        ;;
    list-avds)
        [ -x "$EMULATOR_BIN" ] || {
            echo "✗ emulator not found at $EMULATOR_BIN"
            exit 1
        }
        "$EMULATOR_BIN" -list-avds
        ;;
    create-avd)
        [ -x "$AVDMANAGER" ] || {
            echo "✗ avdmanager not found at $AVDMANAGER"
            exit 1
        }
        mkdir -p "$HOME/.android/avd"
        echo "no" | "$AVDMANAGER" create avd -n "$AVD_NAME" -k "$SYSTEM_IMAGE" -d "$DEVICE_PROFILE" --force
        ;;
    start-emulator)
        [ -x "$EMULATOR_BIN" ] || {
            echo "✗ emulator not found at $EMULATOR_BIN"
            exit 1
        }
        nohup "$EMULATOR_BIN" -avd "$AVD_NAME" -netdelay none -netspeed full > /tmp/${AVD_NAME}.log 2>&1 &
        echo "→ emulator starting; log: /tmp/${AVD_NAME}.log"
        ;;
    wait-device)
        adb_wait
        echo "→ emulator booted"
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
        "$ADB_BIN" shell monkey -p "$APP_ID" -c android.intent.category.LAUNCHER 1 > /dev/null 2>&1 || {
            echo "✗ failed to launch $APP_ID"
            exit 1
        }
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
    doctor)
        echo "ROOT=$ROOT"
        echo "APP_ID=$APP_ID"
        echo "APK_PATH=$APK_PATH"
        echo "SCREENSHOT_DIR=$SCREENSHOT_DIR"
        echo "AVD_NAME=$AVD_NAME"
        echo "ANDROID_HOME=$ANDROID_HOME"
        echo "ANDROID_AVD_HOME=${ANDROID_AVD_HOME:-<unset>}"
        echo "SDKMANAGER=$SDKMANAGER"
        echo "AVDMANAGER=$AVDMANAGER"
        echo "EMULATOR_BIN=$EMULATOR_BIN"
        echo "ADB_BIN=$ADB_BIN"
        [ -x "$ADB_BIN" ] && "$ADB_BIN" devices || true
        ;;
    full)
        BASE="${2:-ontrack}"
        COUNT="${3:-3}"
        "$0" wait-device
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
