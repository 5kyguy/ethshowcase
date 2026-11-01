#!/usr/bin/env bash
# Publish the current dataset snapshot as a GitHub release.
#
# Assets:
#   ethshowcase.db          SQLite + FTS5 (query locally or over the API)
#   projects.jsonl.gz       full normalized snapshot
#   events.json             event metadata
#
# Usage: scripts/release-dataset.sh vX.Y [notes.md]
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
tag="${1:?usage: release-dataset.sh <tag> [notes.md]}"
notes="${2:-}"

for f in data/ethshowcase.db data/projects.jsonl data/events.json; do
  [ -f "$f" ] || { echo "missing $f — run index/details/build first" >&2; exit 1; }
done

gzip -kf data/projects.jsonl
gh release create "$tag" \
  data/ethshowcase.db \
  data/projects.jsonl.gz \
  data/events.json \
  ${notes:+--notes-file "$notes"} \
  --title "Dataset $tag" \
  --generate-notes
echo "released $tag"
