#!/usr/bin/env bash
# Publish the current dataset snapshot as a GitHub release.
#
# Assets:
#   forklore.db          SQLite + FTS5 (query locally or over the API)
#   projects.jsonl.gz       built snapshot (past-only, matches the db)
#   events.json             event metadata
#
# Usage: scripts/release-dataset.sh vX.Y [notes.md]
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
tag="${1:?usage: release-dataset.sh <tag> [notes.md]}"
notes="${2:-}"

for f in data/forklore.db data/projects.built.jsonl data/events.built.json; do
  [ -f "$f" ] || { echo "missing $f — run full-run.sh (build --emit-projects) first" >&2; exit 1; }
done

gzip -kc data/projects.built.jsonl > data/projects.jsonl.gz
gh release create "$tag" \
  data/forklore.db \
  data/projects.jsonl.gz \
  data/events.built.json#events.json \
  ${notes:+--notes-file "$notes"} \
  --title "Dataset $tag" \
  --generate-notes
echo "released $tag"
