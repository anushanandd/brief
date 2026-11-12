#!/bin/sh
# Prints the identity that signs every Brief build, dev and release alike, because Keychain
# trusts saved connections only for the identity that saved them. Developer ID when this Mac has
# it, since a release must carry it; otherwise Apple Development. APPLE_SIGNING_IDENTITY wins.
set -eu

if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
  echo "$APPLE_SIGNING_IDENTITY"
  exit 0
fi
identities="$(/usr/bin/security find-identity -v -p codesigning)"
for kind in "Developer ID Application:" "Apple Development:"; do
  identity="$(printf '%s\n' "$identities" | /usr/bin/awk -F '"' -v kind="$kind" 'index($2, kind) == 1 { print $2; exit }')"
  if [ -n "$identity" ]; then
    echo "$identity"
    exit 0
  fi
done
echo "No Developer ID Application or Apple Development signing identity is in the Keychain." >&2
exit 1
