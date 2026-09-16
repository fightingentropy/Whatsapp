# Apple Silicon packaging

Whatsapp builds `aarch64-apple-darwin` only. `native-packages.macos.yaml`
pins the packaging CLI and describes the `macos-arm64` DMG for
`fightingentropy/Whatsapp`. `packaging/macos/Info.plist` sets the independent
`org.erlin.whatsapp` app identifier and macOS 11 deployment target.
The artifact is `whatsapp-vX.Y.Z-macos-arm64.dmg` and contains
`Whatsapp.app`.

## Local build

On an Apple Silicon Mac with Rust, CMake and Xcode Command Line Tools:

```sh
bash packaging/macos/build-local.sh
```

The helper selects the repository-pinned compiler through rustup, builds the ARM64 release binary, stages and signs the app in a
fresh temporary directory, creates and verifies the DMG, then writes
`dist/Whatsapp-VERSION-local-arm64.dmg`. An optional first argument
chooses another `.dmg` output path. It uses the same bundle and disk-image
recipes as release packaging and requires Ruby and Python 3 in addition to
the build tools. Re-running replaces only the named output DMG.

Temporary staging supports checkouts in iCloud Drive: File Provider can
immediately re-add Finder metadata to a synced `.app`, causing signing to fail.
The finished DMG encloses the signed app, so it can be stored in iCloud.
`bundle.sh` also strips inherited attributes from its freshly generated bundle.

Local test packages are ad-hoc signed and not notarized. For a Developer ID
bundle, call `bundle.sh` with `CODESIGN_IDENTITY` in an unsynced directory.
The public release workflow below uses the native-packages CLI at version 0.5.1.

## Release workflow

After all CI checks pass, update the package version and tag `vX.Y.Z` when a
release is wanted. `.github/workflows/release.yml` builds one ARM64 binary,
creates a DMG, verifies its signature and notarization, then publishes it and
`checksums.txt` to Whatsapp's GitHub release. Missing signing credentials do
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
also targets Whatsapp. Unused non-Mac installers and the inherited website have
been removed. App artwork is generated from `assets/brand/whatsapp-mark.svg`:
run `cargo run --locked --example render_icon` before rebuilding the bundle.
