#!/bin/sh
# Builds Brief and replaces /Applications/Brief.app with it: `pnpm release`.
#
# Signed with the same Apple Development identity as dev builds, so Keychain keeps trusting the
# app and saved connections survive. The running copy is quit first: it holds the data lock, and
# a bundle shouldn't be replaced under a running app.
#
# A release is reversible: it builds only a committed tree (BRIEF_RELEASE_DIRTY=1 overrides),
# copies the finance database aside before the new build first opens it, keeps the old app as
# Brief.app.previous, and tags the commit it built (release-YYYYMMDD, local only, never pushed).
set -eu

cd "$(dirname "$0")/.."
app=/Applications/Brief.app
built=src-tauri/target/release/bundle/macos/Brief.app
data="$HOME/Library/Application Support/com.brief.finance"
kept_copies=5

if [ -n "$(git status --porcelain --untracked-files=all)" ] && [ "${BRIEF_RELEASE_DIRTY:-}" != 1 ]; then
  echo "The tree has uncommitted or untracked files, so this release couldn't be traced to a commit." >&2
  echo "Commit first, or set BRIEF_RELEASE_DIRTY=1 to release it anyway." >&2
  exit 1
fi
commit="$(git rev-parse --short HEAD)"
[ -z "$(git status --porcelain --untracked-files=all)" ] || commit="$commit-dirty"

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

/usr/bin/open "$app"
echo "Brief is updated ($tagged)."
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
