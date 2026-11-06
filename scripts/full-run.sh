#!/usr/bin/env bash
# Full dataset pipeline: index → details → build (DB + static site) → stats.
#
# Idempotent and cache-fed: safe to rerun after any interruption; completed
# phases replay from data/cache without touching ETHGlobal.
#
# Run detached so it survives agent/session restarts:
#   setsid nohup scripts/full-run.sh > data/full-run.log 2>&1 < /dev/null &
#
# Progress: tail -f data/full-run.log
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

BIN=target/release/ethshowcase
[ -x "$BIN" ] || cargo build --release

echo "=== index $(date -u '+%FT%TZ')"
"$BIN" index
echo "=== details $(date -u '+%FT%TZ')"
"$BIN" details
echo "=== details retry pass $(date -u '+%FT%TZ')"
"$BIN" details
echo "=== build $(date -u '+%FT%TZ')"
"$BIN" build --site site/dist --emit-projects data/projects.built.jsonl
echo "=== stats $(date -u '+%FT%TZ')"
"$BIN" stats
echo "=== done $(date -u '+%FT%TZ')"
