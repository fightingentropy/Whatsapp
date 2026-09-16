#!/bin/bash
# Build a local test DMG, even when the checkout lives in iCloud Drive.
# Usage: packaging/macos/build-local.sh [output.dmg]
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
staging="$(mktemp -d -t whatsapp-package)"
trap 'rm -rf "$staging"' EXIT
cd "$root"

# Call rustup directly: a separate Homebrew cargo can otherwise ignore the pin.
toolchain="$(awk -F '\"' '/^channel[[:space:]]*=/ { print $2; exit }' rust-toolchain.toml)"
toolchain_bin="$(dirname "$(rustup which --toolchain "$toolchain" rustc)")"
export PATH="$toolchain_bin:$PATH"
export RUSTC="$toolchain_bin/rustc"
export RUSTDOC="$toolchain_bin/rustdoc"
rustup run "$toolchain" cargo build --locked --release
rustup run "$toolchain" cargo metadata --locked --no-deps --format-version 1 > "$staging/metadata.json"
version="$(python3 -c 'import json,sys; print(next(p["version"] for p in json.load(open(sys.argv[1]))["packages"] if p["name"] == "whatsapp"))' "$staging/metadata.json")"
target_dir="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["target_directory"])' "$staging/metadata.json")"
output="${1:-$root/dist/Whatsapp-$version-local-arm64.dmg}"
case "$output" in
    *.dmg) ;;
    *) echo "Output must be a .dmg path" >&2; exit 1 ;;
esac

# Signing must happen outside synced directories: File Provider can restore
# Finder metadata immediately after xattr clears it. The finished DMG is safe
# to copy back because its signed app stays enclosed in the disk image.
mkdir -p "$staging/input"
CODESIGN_IDENTITY= bash packaging/macos/bundle.sh \
    "$target_dir/aarch64-apple-darwin/release/whatsapp" \
    "$staging/input/Whatsapp.app" "$version"
cp README.md LICENSE "$staging/input/"
ruby packaging/macos/dmg.rb "$staging/input" "$staging/local.dmg"
mkdir -p "$(dirname "$output")"
cp "$staging/local.dmg" "$output"
echo "Local test DMG (ad-hoc signed, not notarized): $output"
