#!/usr/bin/env bash

# test.sh
# ----------------------------------------
cd crates/ontrack-mobile/android
gradle clean :app:assembleRelease
cd ../../..
./scripts/acli.sh full ontrack 4
