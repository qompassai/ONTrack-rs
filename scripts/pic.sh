#!/usr/bin/env bash

# pic.sh
# Qompass AI - [ ]
# Copyright (C) 2026 Qompass AI, All rights reserved
# ----------------------------------------
./acli.sh setup-sdk
./acli.sh create-avd
./acli.sh start-emulator
./acli.sh full ontrack 4

./geny.sh devices
./geny.sh install
./geny.sh launch
./geny.sh screenshot ontrack-home
