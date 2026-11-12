#!/bin/sh
# Builds Brief and replaces /Applications/Brief.app with it: `pnpm release`. With `--publish`,
# also ships it: `pnpm release --publish` notarizes the build and publishes it as GitHub Release
# v<version> (the version in src-tauri/tauri.conf.json), which installed copies update to.
#
# Signed with the identity scripts/signing-identity.sh picks for dev builds too, so Keychain
# keeps trusting the app and saved connections survive. The running copy is quit first: it holds
# the data lock, and a bundle shouldn't be replaced under a running app.
#
# A release is reversible: it builds only a committed tree (BRIEF_RELEASE_DIRTY=1 overrides,
# except when publishing), copies the finance database aside before the new build first opens
# it, keeps the old app as Brief.app.previous, and tags the commit it built: v<version> on GitHub
# when published, otherwise release-YYYYMMDD, local only.
#
# Inputs outside the repo, in ~/.brief-private/:
#   updater.key   the updater's signing key (`pnpm tauri signer generate`), whose password is
#                 the login Keychain item brief-updater-key. Every build signs its update with
#                 it; lose it and installed copies can never update again, so keep a backup.
#   notary.env    for --publish: APPLE_API_KEY, APPLE_API_ISSUER, and APPLE_API_KEY_PATH, an
#                 App Store Connect API key that notarizes the app and the disk image.
set -eu

cd "$(dirname "$0")/.."
app=/Applications/Brief.app
bundle=src-tauri/target/release/bundle
built=$bundle/macos/Brief.app
data="$HOME/Library/Application Support/com.brief.finance"
private="$HOME/.brief-private"
repo=anushanandd/brief
kept_copies=5

publish=false
case "${1:-}" in
  --publish) publish=true ;;
  "") ;;
  *)
    echo "Usage: pnpm release [--publish]" >&2
    exit 2
    ;;
esac

if [ -n "$(git status --porcelain --untracked-files=all)" ] &&
  { [ "$publish" = true ] || [ "${BRIEF_RELEASE_DIRTY:-}" != 1 ]; }; then
  echo "The tree has uncommitted or untracked files, so this release couldn't be traced to a commit." >&2
  [ "$publish" = true ] || echo "Commit first, or set BRIEF_RELEASE_DIRTY=1 to release it anyway." >&2
  exit 1
fi
commit="$(git rev-parse --short HEAD)"
[ -z "$(git status --porcelain --untracked-files=all)" ] || commit="$commit-dirty"
version="$(/usr/bin/plutil -extract version raw -o - src-tauri/tauri.conf.json)"
identity="$(sh scripts/signing-identity.sh)"

if [ ! -f "$private/updater.key" ]; then
  echo "No updater key at $private/updater.key; see this script's header." >&2
  exit 1
fi
TAURI_SIGNING_PRIVATE_KEY="$private/updater.key"
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(/usr/bin/security find-generic-password -s brief-updater-key -w)"
export TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD

bundles=app
if [ "$publish" = true ]; then
  # Everything that could stop a publish is checked before the build, which takes minutes.
  case "$identity" in
    "Developer ID Application:"*) ;;
    *)
      echo "Publishing needs a Developer ID Application certificate; this Mac signs with $identity." >&2
      exit 1
      ;;
  esac
  if [ ! -f "$private/notary.env" ]; then
    echo "No $private/notary.env; see this script's header." >&2
    exit 1
  fi
  set -a
  . "$private/notary.env"
  set +a
  : "${APPLE_API_KEY:?notary.env sets no APPLE_API_KEY}" "${APPLE_API_ISSUER:?notary.env sets no APPLE_API_ISSUER}"
  if [ ! -f "${APPLE_API_KEY_PATH:-}" ]; then
    echo "notary.env's APPLE_API_KEY_PATH names no file." >&2
    exit 1
  fi
  git fetch -q origin main --tags
  if ! git merge-base --is-ancestor HEAD origin/main; then
    echo "Push $commit to main first, so the release's tag points at code GitHub has." >&2
    exit 1
  fi
  if git rev-parse -q --verify "refs/tags/v$version" >/dev/null || gh release view "v$version" -R "$repo" >/dev/null 2>&1; then
    echo "v$version is already released. Raise the version in src-tauri/tauri.conf.json first." >&2
    exit 1
  fi
  bundles=app,dmg
fi

echo "Building Brief $version, signed by $identity. After code changes the Rust release build takes several minutes…"
APPLE_SIGNING_IDENTITY="$identity" pnpm tauri build --bundles "$bundles"

if [ "$publish" = true ]; then
  # Tauri notarized and stapled the app; this confirms Gatekeeper will open it.
  /usr/sbin/spctl --assess --type execute "$built"
  dmg="$(/bin/ls "$bundle"/dmg/Brief_"$version"_*.dmg)"
  echo "Notarizing $dmg…"
  /usr/bin/xcrun notarytool submit "$dmg" --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" \
    --issuer "$APPLE_API_ISSUER" --wait
  # Stapling fails unless Apple accepted the disk image, so a rejected one stops here.
  /usr/bin/xcrun stapler staple "$dmg"

  work="$(/usr/bin/mktemp -d)"
  trap '/bin/rm -rf "$work"' EXIT
  previous="$(git describe --tags --abbrev=0 --match 'v*' 2>/dev/null || true)"
  git log --format='- %s' "${previous:+$previous..}HEAD" >"$work/notes.md"
  tarball="$bundle/macos/Brief.app.tar.gz"
  /usr/bin/jq -n \
    --arg version "$version" \
    --rawfile notes "$work/notes.md" \
    --arg date "$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --arg signature "$(/bin/cat "$tarball.sig")" \
    --arg url "https://github.com/$repo/releases/download/v$version/Brief.app.tar.gz" \
    '{version: $version, notes: $notes, pub_date: $date,
      platforms: {"darwin-aarch64": {signature: $signature, url: $url}}}' >"$work/latest.json"

  # A draft holds the files until all are uploaded, so the latest release never names a file
  # that isn't there yet. Publishing it creates the tag on GitHub.
  echo "Publishing v$version…"
  gh release create "v$version" -R "$repo" --draft --target "$(git rev-parse HEAD)" \
    --title "Brief $version" --notes-file "$work/notes.md" "$dmg" "$tarball" "$work/latest.json"
  gh release edit "v$version" -R "$repo" --draft=false --latest
  git fetch -q origin tag "v$version"
  tagged="published v$version: https://github.com/$repo/releases/tag/v$version"
fi

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

# Copy the database while nothing holds it: a copy of an open SQLite file can be torn.
if /usr/bin/pgrep -xq brief; then
  echo "Brief started again; its database can't be copied safely. Quit it, then run pnpm release again." >&2
  exit 1
fi
copy=""
if [ -f "$data/finance-state.sqlite3" ]; then
  copy="$data/releases/pre-release-$commit-$(/bin/date +%Y%m%d-%H%M%S)"
  echo "Copying the finance database to ${copy}…"
  /bin/mkdir -p "$copy"
  /bin/chmod 700 "$data/releases" "$copy"
  for file in "$data"/finance-state.sqlite3 "$data"/finance-state.sqlite3-wal \
    "$data"/finance-state.backup.sqlite3 "$data"/finance-state.backup.sqlite3-wal; do
    if [ -f "$file" ]; then /bin/cp -p "$file" "$copy/"; fi
  done
  # Keep the newest copies; each is a complete copy of your data.
  /bin/ls -dt "$data"/releases/pre-release-* | /usr/bin/tail -n +$((kept_copies + 1)) |
    while IFS= read -r old; do /bin/rm -rf "$old"; done
fi

echo "Installing to ${app}…"
# Copy beside the old app, then swap, so a failed copy never leaves no app at all. The old app
# stays as Brief.app.previous until the next release.
/bin/rm -rf "$app.new"
/usr/bin/ditto "$built" "$app.new"
if [ -d "$app" ]; then
  /bin/rm -rf "$app.previous"
  /bin/mv "$app" "$app.previous"
fi
/bin/mv "$app.new" "$app"

if [ "$publish" = false ]; then
  tag="release-$(/bin/date +%Y%m%d)"
  if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    n=2
    while git rev-parse -q --verify "refs/tags/$tag-$n" >/dev/null; do n=$((n + 1)); done
    tag="$tag-$n"
  fi
  if git tag -a "$tag" -m "Brief released to $app from $commit"; then
    tagged="tagged $tag"
  else
    tagged="not tagged; git tag failed"
  fi
fi

/usr/bin/open "$app"
echo "Brief $version is installed ($tagged)."
echo
echo "To roll back: quit Brief, then"
if [ -d "$app.previous" ]; then
  echo "  rm -rf '$app' && mv '$app.previous' '$app'"
fi
if [ -n "$copy" ]; then
  echo "  and, only if the new build changed the database (this replaces it):"
  echo "  rm -f '$data'/finance-state*.sqlite3-wal '$data'/finance-state*.sqlite3-shm"
  echo "  cp -p '$copy'/* '$data/'"
fi
