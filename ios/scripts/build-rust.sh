#!/bin/bash
set -euo pipefail
project_root="$(cd "$(dirname "$0")/../.." && pwd)"
platform="${PLATFORM_NAME:-iphonesimulator}"
case "$platform" in
  iphoneos) rust_target=aarch64-apple-ios ;;
  iphonesimulator) rust_target=aarch64-apple-ios-sim ;;
  *) echo "Unsupported iOS platform: $platform" >&2; exit 1 ;;
esac
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin"
rustup_bin="$(command -v rustup)"
toolchain_bin="$(dirname "$("$rustup_bin" which --toolchain 1.98.0 rustc)")"
export PATH="$toolchain_bin:$PATH"
export RUSTC="$toolchain_bin/rustc"
export RUSTDOC="$toolchain_bin/rustdoc"
export DYLD_FALLBACK_LIBRARY_PATH="$toolchain_bin/../lib${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"
export IPHONEOS_DEPLOYMENT_TARGET=18.0
cd "$project_root"
# Let cc/cmake choose each host or target SDK rather than applying Xcode's iOS
# SDK to host-side Rust build scripts and procedural macros.
unset SDKROOT
cargo build --locked -p whatsapp-ios-core --target "$rust_target" --release
mkdir -p "$project_root/ios/build/$platform"
cp "$project_root/target/$rust_target/release/libwhatsapp_ios_core.a" "$project_root/ios/build/$platform/"
