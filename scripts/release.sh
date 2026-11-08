#!/bin/sh
# Builds Brief and replaces /Applications/Brief.app with it: `pnpm release`.
#
# Signed with the same Apple Development identity as dev builds, so Keychain keeps trusting the
# app and saved connections survive. The running copy is quit first: it holds the data lock, and
# a bundle shouldn't be replaced under a running app.
set -eu

cd "$(dirname "$0")/.."
app=/Applications/Brief.app
built=src-tauri/target/release/bundle/macos/Brief.app

identity="${APPLE_SIGNING_IDENTITY:-$(/usr/bin/security find-identity -v -p codesigning | /usr/bin/awk -F '"' '/Apple Development:/ { print $2; exit }')}"
: "${identity:?No Apple Development signing identity found. Set APPLE_SIGNING_IDENTITY.}"

echo "Building Brief, signed by $identity. After code changes the Rust release build takes several minutes…"
APPLE_SIGNING_IDENTITY="$identity" pnpm tauri build

if /usr/bin/pgrep -xq brief; then
  echo "Quitting Brief…"
  /usr/bin/osascript -e 'quit app "Brief"' || true
  # Quitting lets a refresh finish; wait up to 30 seconds rather than force it.
  attempts=0
  while /usr/bin/pgrep -xq brief; do
    attempts=$((attempts + 1))
    if [ "$attempts" -ge 60 ]; then
      echo "Brief is still running. Quit it, then run pnpm release again." >&2
      exit 1
    fi
    /bin/sleep 0.5
  done
fi

echo "Installing to ${app}…"
# Copy beside the old app, then swap, so a failed copy never leaves no app at all.
/bin/rm -rf "$app.new"
/usr/bin/ditto "$built" "$app.new"
/bin/rm -rf "$app"
/bin/mv "$app.new" "$app"

/usr/bin/open "$app"
echo "Brief is updated."
