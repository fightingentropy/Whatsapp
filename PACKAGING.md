# Apple Silicon packaging

This fork builds `aarch64-apple-darwin` only. `native-packages.macos.yaml`
pins the packaging CLI and describes the `macos-arm64` DMG for
`fightingentropy/zapfast`. `packaging/macos/Info.plist` sets the independent
`org.erlin.zapfast-silicon` app identifier and macOS 11 deployment target.
The artifact is `zapfast-vX.Y.Z-macos-arm64.dmg` and contains
`ZapFast Silicon.app`.

## Local build

On an Apple Silicon Mac with Rust, CMake and Xcode Command Line Tools:

```sh
cargo build --locked --release
bash packaging/macos/bundle.sh target/aarch64-apple-darwin/release/zapfast \
  "dist/macos-input/ZapFast Silicon.app" 0.13.1
cp README.md LICENSE dist/macos-input/
gem install native-packages --version 0.5.1 --no-document
native-packages --config native-packages.macos.yaml build \
  --version 0.13.1 --target macos-arm64 --output dist/macos-packages-test
```

`bundle.sh` validates the input architecture, includes microphone permission
metadata, and signs the app ad hoc by default. `CODESIGN_IDENTITY` enables a
Developer ID signature. An ad-hoc signature is for local testing; it does not
establish notarization or Gatekeeper distribution acceptance.

## Release workflow

After all CI checks pass, update the package version and tag `vX.Y.Z` when a
release is wanted. `.github/workflows/release.yml` builds one ARM64 binary,
creates a DMG, verifies its signature and notarization, then publishes it and
`checksums.txt` to this fork's GitHub release. Missing signing credentials do
not produce a successful public release: the final notarization check fails.
No Linux/Windows packages, AUR recipes, upstream tap or website are published.

The packaging tool needs this repository's own secrets:

- `APPLE_CERTIFICATE_P12`: base64 Developer ID Application certificate/private key.
- `APPLE_CERTIFICATE_PASSWORD`: export password.
- `APPLE_SIGNING_IDENTITY`: exact Developer ID Application identity.
- `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD`: notarization credentials.

Never commit these values. The package tool signs its owned input copy and
manages a temporary keychain. `verify.sh` checks the final DMG's stapled ticket,
Gatekeeper acceptance, ARM64-only architecture and microphone metadata.
See the pinned tool's
[Apple setup instructions](https://github.com/crmne/native-packages/blob/v0.5.1/docs/apple-notarization.md).

`native-packages.yaml` links to the same Mac configuration so the CLI default
also targets this fork. Non-Mac recipes remain as upstream reference material
and are unused by the workflows.
