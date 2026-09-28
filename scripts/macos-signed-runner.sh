#!/bin/sh
set -eu

binary="$1"
shift
identity="${APPLE_SIGNING_IDENTITY:-$(/usr/bin/security find-identity -v -p codesigning | /usr/bin/awk -F '"' '/Apple Development:/ { print $2; exit }')}"
: "${identity:?No Apple Development signing identity found. Set APPLE_SIGNING_IDENTITY.}"

/usr/bin/codesign --force --sign "$identity" --identifier com.brief.finance --timestamp=none "$binary"
exec "$binary" "$@"
