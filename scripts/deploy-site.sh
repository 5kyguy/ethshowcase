#!/usr/bin/env bash
# Deploy the generated static site (site/dist) to GitHub Pages.
#
# Prerequisites:
#   - `forklore build --site site/dist` has been run
#   - origin remote is the forklore repo, with push access
#
# Usage: scripts/deploy-site.sh
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

if [ ! -f site/dist/index.html ]; then
  echo "site/dist/index.html missing — run: forklore build --site site/dist" >&2
  exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

git worktree add --detach "$work" >/dev/null
(
  cd "$work"
  git checkout --orphan gh-pages 2>/dev/null || git checkout -B gh-pages
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
# Drop the local ref so reruns start clean.
git branch -D gh-pages >/dev/null 2>&1 || true

# gh-pages is an orphan branch without the workflow file, so the push itself
# can't trigger it — dispatch the deploy workflow (runs on main's copy).
if gh workflow run deploy-site.yml --ref main 2>/dev/null; then
  echo "pushed gh-pages — deploy workflow dispatched (Pages builds via GitHub Actions)"
else
  echo "pushed gh-pages — could not dispatch workflow (gh auth?); run it manually:" >&2
  echo "  gh workflow run deploy-site.yml --ref main" >&2
fi
