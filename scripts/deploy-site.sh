#!/usr/bin/env bash
# Deploy the generated static site (site/dist) to GitHub Pages.
#
# Prerequisites:
#   - `ethshowcase build --site site/dist` has been run
#   - origin remote is the ethshowcase repo, with push access
#
# Usage: scripts/deploy-site.sh
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

if [ ! -f site/dist/index.html ]; then
  echo "site/dist/index.html missing — run: ethshowcase build --site site/dist" >&2
  exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

git worktree add --detach "$work" >/dev/null
(
  cd "$work"
  git checkout --orphan gh-pages
  git rm -rf --quiet . 2>/dev/null || true
  git clean -fdq
  cp -r "$OLDPWD"/site/dist/. .
  # Pages needs Jekyll disabled so underscore/data dirs serve as-is.
  touch .nojekyll
  git add -A
  git commit -q -m "deploy: static site $(date -u '+%Y-%m-%dT%H:%MZ')"
)
git push -f origin gh-pages
git worktree remove --force "$work"

echo "pushed gh-pages — enable Pages: Settings → Pages → Source: gh-pages branch"
