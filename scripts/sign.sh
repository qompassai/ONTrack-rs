#!/usr/bin/env bash

# sign.sh
# Qompass AI - [ ]
# Copyright (C) 2026 Qompass AI, All rights reserved
# ----------------------------------------
apksigner verify --print-certs crates/ontrack-mobile/android/app/build/outputs/apk/release/app-release.apk

jarsigner -verify -verbose -certs crates/ontrack-mobile/android/app/build/outputs/bundle/release/app-release.aab
