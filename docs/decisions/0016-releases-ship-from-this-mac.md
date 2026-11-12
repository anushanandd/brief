# 0016: Releases ship from this Mac through GitHub Releases

2026-11-12 · Accepted

## Context

Brief was installed only by `pnpm release` on its developer's Mac. Shipping it to anyone else needs a notarized download and a way to update. The options were Sparkle or Tauri's updater for updates, a CI workflow or this Mac for builds, and a host for the files.

## Decision

- **Tauri's updater, not Sparkle.** It ships with Tauri, verifies each update against a public key in `tauri.conf.json` with a key only this Mac holds (`~/.brief-private/updater.key`), and reads a static `latest.json`. Sparkle would mean bridging a native framework into a Tauri app; its delta updates save little on a 20 MB download.
- **GitHub Releases host everything.** Each release `v<version>` carries the notarized disk image, the signed update archive, and `latest.json`. The updater reads `releases/latest/download/latest.json`, which GitHub serves from the newest published release. A release stays a draft until every file is uploaded.
- **`pnpm release --publish` builds on this Mac, not in CI.** The Developer ID certificate, the notarization key, and the updater key never leave the Mac, and the build uses this Mac's Xcode. CI runners lag a major Xcode version behind.
- **One signing identity for every build.** `scripts/signing-identity.sh` picks Developer ID when the Keychain has it, otherwise Apple Development, for dev builds and releases alike, because Keychain trusts saved connections only for the identity that saved them.
- **The app checks at launch and daily, and installs only when asked.** Rust does the check and the install (`app_update.rs`), so the webview gets two commands rather than the updater plugin's permissions. Debug builds never update.

## Consequences

- Losing `updater.key` or its password (the Keychain item `brief-updater-key`) strands every installed copy on its version. Both need a backup outside this Mac.
- The update check is a request to GitHub that Brief makes on its own, carrying no data. The README's privacy section says so.
- Switching from Apple Development to Developer ID changes the app's identity once, so Keychain asks about saved connections once.
- A release is Apple silicon only (`darwin-aarch64`), like the Macs Apple Intelligence runs on.
