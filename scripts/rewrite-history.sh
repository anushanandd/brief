#!/bin/sh
# Rewrites history to remove personal strings and files before the repo goes public. The owner
# runs this by hand, once, in a fresh clone; never in the working repo, and never from an agent.
#
#   git clone --no-local ~/brief ~/brief-clean && cd ~/brief-clean
#   git branch v2 origin/v2                     # every branch to keep must exist locally
#   sh scripts/rewrite-history.sh --i-am-in-a-fresh-clone
#
# Inputs, all outside the repo in ~/.brief-private/:
#   replacements.txt   git filter-repo --replace-text expressions (`literal==>replacement`,
#                      `regex:…`, `glob:…`), applied to file contents and commit messages
#   paths-to-drop.txt  paths removed from every commit (optional; filter-repo --paths-from-file)
#   mailmap            author and committer rewrites (optional; git mailmap format)
#   denylist.txt       strings that must not survive; checked by scripts/scan-history.sh
set -eu

private="$HOME/.brief-private"
replacements="$private/replacements.txt"
paths="$private/paths-to-drop.txt"
mailmap="$private/mailmap"

if [ "${1:-}" != --i-am-in-a-fresh-clone ]; then
  echo "This rewrites every commit. Run it only in a fresh clone, with --i-am-in-a-fresh-clone." >&2
  exit 2
fi
cd "$(git rev-parse --show-toplevel)"
if [ -n "$(git status --porcelain --untracked-files=all)" ]; then
  echo "The worktree has changes or untracked files, so this isn't a fresh clone. Stopping." >&2
  exit 1
fi
if ! git filter-repo --version >/dev/null 2>&1; then
  echo "git filter-repo isn't installed: brew install git-filter-repo" >&2
  exit 1
fi
if [ ! -f "$replacements" ]; then
  echo "No $replacements; nothing to replace." >&2
  exit 1
fi

set -- --replace-text "$replacements" --replace-message "$replacements"
if [ -f "$paths" ]; then
  set -- "$@" --invert-paths --paths-from-file "$paths"
else
  echo "No $paths; no files dropped."
fi
if [ -f "$mailmap" ]; then
  set -- "$@" --mailmap "$mailmap"
else
  echo "No $mailmap; authors unchanged."
fi

# filter-repo refuses a repo that isn't a fresh clone; don't override that with --force.
git filter-repo "$@"

echo
echo "Checking the rewritten history against the denylist…"
sh scripts/scan-history.sh

cat <<'EOF'

History is clean. Next, by hand:
  1. On GitHub, delete anushanandd/brief and create it again, empty (a force-push leaves the old
     commits reachable by hash, in forks, and in cached pull requests).
  2. Here: git remote add origin https://github.com/anushanandd/brief.git
           git push -u origin main v2
  3. In the old working repo, drop refs that still hold the old history, then prune:
       git for-each-ref --format='%(refname)' refs/codex | xargs -n 1 git update-ref -d
       git stash list   # move anything worth keeping first; then git stash clear
       git reflog expire --expire=now --all && git gc --prune=now
     or, simpler, move your uncommitted work into this clone and retire the old one.
EOF
