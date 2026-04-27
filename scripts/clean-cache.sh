#!/usr/bin/env bash
# Reclaim disk space from build caches and old reports.
# Run: bash scripts/clean-cache.sh
# Or:  make clean
set -e

cd "$(dirname "$0")/.."
ROOT="$(pwd)"

echo "Cleaning Rust build cache..."
cargo clean 2>/dev/null || true

if [ -d node_modules ]; then
  echo "Removing legacy JS node_modules..."
  rm -rf node_modules
fi

if [ -d infra/cloudflare/node_modules ]; then
  echo "Removing infra/cloudflare/node_modules (regen with 'npm install' on next deploy)..."
  rm -rf infra/cloudflare/node_modules
fi

if [ -d .omx ]; then
  echo "Removing .omx state..."
  rm -rf .omx
fi

if [ -d reports ]; then
  echo "Trimming reports/ — keeping latest-battle.* and the 3 most recent timestamped pairs..."
  cd reports
  ls -t 2026-*.html 2025-*.html 2027-*.html 2>/dev/null | tail -n +4 | while read -r f; do
    rm -f "$f" "${f%.html}.json"
  done
  ls -td */ 2>/dev/null | tail -n +4 | xargs -I{} rm -rf {} 2>/dev/null || true
  cd "$ROOT"
fi

echo
echo "Done. Current size:"
du -sh "$ROOT"
