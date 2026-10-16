#!/bin/sh
set -eu

binary="$1"
shift

# Brief runs from `$binary` itself: the Keychain trusts the app at this path, so a copy
# elsewhere loses access to saved connections.
#
# Cargo copies the build from `deps/` over `$binary` on every run, and a freshly signed binary
# costs most of a second at launch while macOS assesses it. So sign the copy's source once per
# build; every later run then copies an already-signed binary, with nothing to assess.
signed() {
  /usr/bin/codesign -dv --verbose=2 "$1" 2>&1 | /usr/bin/grep -q '^Authority=Apple Development:'
}
if ! signed "$binary"; then
  identity="${APPLE_SIGNING_IDENTITY:-$(/usr/bin/security find-identity -v -p codesigning | /usr/bin/awk -F '"' '/Apple Development:/ { print $2; exit }')}"
  : "${identity:?No Apple Development signing identity found. Set APPLE_SIGNING_IDENTITY.}"
  size="$(/usr/bin/stat -f %z "$binary")"
  for candidate in "$(dirname "$binary")"/deps/"$(basename "$binary")"-*; do
    case "$candidate" in *.*) continue ;; esac
    if [ "$(/usr/bin/stat -f %z "$candidate")" = "$size" ] && /usr/bin/cmp -s "$candidate" "$binary"; then
      /usr/bin/codesign --force --sign "$identity" --identifier com.brief.finance --timestamp=none "$candidate"
      break
    fi
  done
  /usr/bin/codesign --force --sign "$identity" --identifier com.brief.finance --timestamp=none "$binary"
fi

# Tauri starts this alongside the dev server rather than after it (TAURI_CLI_NO_DEV_SERVER_WAIT),
# so wait here, briefly and often, for the window's first load to have something to load.
if [ -n "${TAURI_CLI_NO_DEV_SERVER_WAIT:-}" ]; then
  attempts=0
  until /usr/bin/curl -s -o /dev/null --max-time 1 http://localhost:1420/; do
    attempts=$((attempts + 1))
    [ "$attempts" -lt 400 ] || break
    /bin/sleep 0.05
  done
fi
exec "$binary" "$@"
