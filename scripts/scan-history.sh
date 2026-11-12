#!/bin/sh
# Searches every commit on every ref for the strings in the private denylist: file contents,
# file paths, and commit messages, case-insensitively. Read-only. Prints hits by entry number
# (line number among the denylist's non-comment lines), never the strings themselves, and exits
# non-zero on any hit. Run it after scripts/rewrite-history.sh, and before making the repo public.
#
#   sh scripts/scan-history.sh [denylist]     # default: ~/.brief-private/denylist.txt
set -eu

denylist="${1:-${BRIEF_DENYLIST:-$HOME/.brief-private/denylist.txt}}"
if [ ! -f "$denylist" ]; then
  echo "No denylist at $denylist (one string per line, # for comments)." >&2
  exit 2
fi
cd "$(git rev-parse --show-toplevel)"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
chmod 700 "$work"
# The denylist without blank lines, comments, or carriage returns.
sed -e 's/\r$//' -e '/^#/d' -e '/^$/d' "$denylist" >"$work/all"
total="$(wc -l <"$work/all" | tr -d ' ')"
# shellcheck disable=SC2046 # one argument per commit
set -- $(git rev-list --all)
echo "Scanning $# commits for $total denylist entries…"

# One pass over every commit's files for all entries; per-entry passes only when it finds any.
any_contents="$(git grep -l -F -i -f "$work/all" "$@" -- 2>/dev/null || true)"
for commit in "$@"; do
  git ls-tree -r --name-only "$commit" | sed "s|^|$commit:|"
done >"$work/paths"

hits=0
index=0
while IFS= read -r entry; do
  index=$((index + 1))
  printf '%s\n' "$entry" >"$work/one"
  contents=""
  if [ -n "$any_contents" ]; then
    contents="$(git grep -l -F -i -f "$work/one" "$@" -- 2>/dev/null || true)"
  fi
  paths="$(grep -F -i -f "$work/one" "$work/paths" || true)"
  messages="$(git log --all --regexp-ignore-case --fixed-strings --grep="$entry" --format=%H)"
  count=0
  for list in "$contents" "$paths" "$messages"; do
    [ -n "$list" ] && count=$((count + $(printf '%s\n' "$list" | wc -l)))
  done
  [ "$count" -eq 0 ] && continue
  hits=$((hits + count))
  echo
  echo "Entry #$index: $count hits"
  # A path is printed only when its own name is clean; the content hit still counts.
  [ -n "$contents" ] && printf '%s\n' "$contents" | while IFS= read -r hit; do
    if printf '%s\n' "$hit" | grep -q -F -i -f "$work/all"; then
      echo "  content: ${hit%%:*}:(path withheld)"
    else
      echo "  content: $hit"
    fi
  done
  [ -n "$paths" ] && printf '%s\n' "$paths" | sed 's|^\([^:]*\):.*|  path:    \1:(path withheld)|' | sort -u
  [ -n "$messages" ] && printf '%s\n' "$messages" | sed 's/^/  message: /'
done <"$work/all"

echo
if [ "$hits" -gt 0 ]; then
  echo "$hits hits. History is not clean." >&2
  exit 1
fi
echo "No hits: no denylist entry appears in any commit's files, paths, or message."
