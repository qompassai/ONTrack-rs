{
  description = "OnTrack-RS — TDS Telecom Field Route Optimizer (pure Rust)";

  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url  = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { nixpkgs, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
          config.android_sdk.accept_license = true;
          config.allowUnfree = true;
        };
        rust = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "clippy" "rust-analyzer" ];
          targets = [
            "aarch64-linux-android"
            "armv7-linux-androideabi"
            "x86_64-unknown-linux-gnu"
            "x86_64-pc-windows-gnu"
          ];
        };

        commonNativeBuildInputs = with pkgs; [
          pkg-config
        ];
        commonBuildInputs = with pkgs; [
          openssl
          fontconfig
          freetype
          libGL
          libxkbcommon
          wayland
          xorg.libX11 xorg.libXcursor xorg.libXi xorg.libXrandr
          alsa-lib
        ];
      in {
        devShells.default = pkgs.mkShell {
          name = "ontrack-rs-linux";
          packages = [ rust pkgs.cargo-ndk pkgs.cmake pkgs.git ] ++ commonNativeBuildInputs ++ commonBuildInputs;
          shellHook = ''
            export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig"
          '';
        };

        devShells.android = pkgs.mkShell {
          name = "ontrack-rs-android";
          packages = with pkgs; [
            rust cargo-ndk cmake git jdk17_headless
            androidenv.androidPkgs.androidsdk
          ] ++ commonNativeBuildInputs;
          shellHook = ''
            export ANDROID_HOME="${pkgs.androidenv.androidPkgs.androidsdk}/libexec/android-sdk"
            export ANDROID_NDK_HOME="$ANDROID_HOME/ndk-bundle"
            echo "ANDROID_HOME=$ANDROID_HOME"
            echo "ANDROID_NDK_HOME=$ANDROID_NDK_HOME"
          '';
        };

        devShells.windows = pkgs.mkShell {
          name = "ontrack-rs-windows";
          packages = with pkgs; [
            rust
            pkgsCross.mingwW64.stdenv.cc
            pkgsCross.mingwW64.windows.pthreads
            cmake git
          ];
        };
      });
}
