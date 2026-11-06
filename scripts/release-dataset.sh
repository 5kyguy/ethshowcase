#!/usr/bin/env bash
# Publish the current dataset snapshot as a GitHub release.
#
# Assets (staged under their final names — gh release uses the basename, the
# file#label syntax only sets a display label):
#   forklore.db        SQLite + FTS5 (query locally or over the API)
#   ethshowcase.db     legacy-name alias of the same bytes (drop after a
#                      couple of releases; old skill installs reference it)
#   projects.jsonl.gz  built snapshot (past-only, matches the db)
#   events.json        event metadata (built, past-only)
#
# Publishing also triggers .github/workflows/deploy-data.yml, which mirrors
# the assets to Cloudflare R2 (forklore-data.skyguy.dev) when the
# CLOUDFLARE_* secrets are set.
#
# Usage: scripts/release-dataset.sh vX.Y [notes.md]
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
tag="${1:?usage: release-dataset.sh <tag> [notes.md]}"
notes="${2:-}"

for f in data/forklore.db data/projects.built.jsonl data/events.built.json; do
  [ -f "$f" ] || { echo "missing $f — run full-run.sh (build --emit-projects) first" >&2; exit 1; }
done

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
gzip -kc data/projects.built.jsonl > "$stage/projects.jsonl.gz"
cp data/forklore.db "$stage/forklore.db"
cp data/forklore.db "$stage/ethshowcase.db"
cp data/events.built.json "$stage/events.json"

gh release create "$tag" \
  "$stage/forklore.db" \
  "$stage/ethshowcase.db" \
  "$stage/projects.jsonl.gz" \
  "$stage/events.json" \
  ${notes:+--notes-file "$notes"} \
  --title "Dataset $tag" \
  --generate-notes
echo "released $tag (R2 sync triggers automatically if Cloudflare secrets are set)"
