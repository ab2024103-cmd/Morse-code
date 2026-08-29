#!/usr/bin/env bash
# Build the Android debug APK and release AAB.
# Prerequisites (a machine with these toolchains):
#   * JDK 17, Android SDK (platform 35 + build-tools 35.0.0 + NDK 26.3.11579264)
#   * Rust stable + Android targets (aarch64-linux-android, armv7-linux-androideabi, x86_64-linux-android)
#   * cargo-ndk, uniffi-bindgen (matching the uniffi version in rust-core/Cargo.toml)
set -euo pipefail

cd "$(dirname "$0")/.."

echo ">> Installing Rust Android targets (idempotent)"
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android || true
cargo install cargo-ndk --locked || true
# The uniffi-bindgen CLI is built by the :core-transfer Gradle task from
# rust-core/bindgen (pinned to uniffi 0.28.0), so nothing to install here.

echo ">> Building native core + Android app"
# If you don't have the NDK toolchain, run with MORSELINK_SKIP_NATIVE=1 to
# compile the Kotlin/UI only (the generated .so must then be added manually).
MORSELINK_SKIP_NATIVE="${MORSELINK_SKIP_NATIVE:-}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/26.3.11579264}"

cd android
gradle assembleDebug bundleRelease --no-daemon --stacktrace

echo
echo ">> APK:  android/app/build/outputs/apk/debug/app-debug.apk"
echo ">> AAB:  android/app/build/outputs/bundle/release/app-release.aab"
