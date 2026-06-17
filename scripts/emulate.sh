#!/usr/bin/env bash

# emulate.sh
# Qompass AI - [ ]
# Copyright (C) 2026 Qompass AI, All rights reserved
# ----------------------------------------
export ANDROID_HOME=/opt/android-sdk
export PATH="$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"
sdkmanager "platform-tools" "emulator" "platforms;android-36" "system-images;android-36;google_apis;x86_64"
avdmanager create avd -n ontrack-pixel -k "system-images;android-36;google_apis;x86_64" -d pixel
emulator -avd ontrack-pixel
adb install -r /home/phaedrus/.GH/Qompass/ONTrack-rs/crates/ontrack-mobile/android/app/build/outputs/apk/release/app-release.apk
adb exec-out screencap -p > ontrack-home.png
