#!/usr/bin/env bash
set -euo pipefail
APP=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$APP/../.." && pwd)
CRATE="$REPO/rust/crates/dvda-android"

cargo build --release --manifest-path "$CRATE/Cargo.toml" --target aarch64-linux-android
mkdir -p "$APP/android/app/src/main/jniLibs/arm64-v8a"
cp "$CRATE/target/aarch64-linux-android/release/libdvda_android.so"   "$APP/android/app/src/main/jniLibs/arm64-v8a/libdvda_android.so"
flutter build apk --release --target-platform android-arm64
