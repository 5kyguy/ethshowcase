#!/usr/bin/env bash
# Sync the current dataset artifacts to Cloudflare R2.
#
# Local twin of .github/workflows/deploy-data.yml — pushes straight from the
# local data/ directory (no release needed):
#   latest/…        overwritten every run (mirrors releases/latest/download)
#   releases/vX.Y/… immutable per-release copies
#
# Requires: CLOUDFLARE_API_TOKEN + CLOUDFLARE_ACCOUNT_ID in the environment
# (or wrangler login), an R2 bucket named "forklore" with the public custom
# domain forklore-data.skyguy.dev connected.
#
# Usage: scripts/deploy-data.sh vX.Y
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
tag="${1:?usage: deploy-data.sh <tag>}"
bucket=forklore

for f in data/forklore.db data/projects.built.jsonl data/events.built.json; do
  [ -f "$f" ] || { echo "missing $f — run full-run.sh first" >&2; exit 1; }
done

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
gzip -kc data/projects.built.jsonl > "$stage/projects.jsonl.gz"

upload() { # key file content-type cache-control
  echo "→ r2://$bucket/$1"
  npx -y wrangler@latest r2 object put "$bucket/$1" \
    --file "$2" --remote --content-type "$3" --cache-control "$4" >/dev/null
}

for prefix in "latest" "releases/$tag"; do
  if [ "$prefix" = "latest" ]; then
    cc="public, max-age=300"
  else
    cc="public, max-age=31536000, immutable"
  fi
  upload "$prefix/forklore.db"       data/forklore.db           application/octet-stream "$cc"
  upload "$prefix/projects.jsonl.gz" "$stage/projects.jsonl.gz" application/gzip         "$cc"
  upload "$prefix/events.json"       data/events.built.json     application/json         "$cc"
done
echo "synced → https://forklore-data.skyguy.dev/latest/ and /releases/$tag/"
