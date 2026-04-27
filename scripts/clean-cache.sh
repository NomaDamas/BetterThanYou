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
  echo "Trimming reports/ — keeping only latest-battle.* and latest-published.json..."
  # Use bash with nullglob so missing patterns don't halt zsh.
  bash -c '
    shopt -s nullglob
    cd reports
    rm -rf -- 2024-*-share/ 2025-*-share/ 2026-*-share/ 2027-*-share/
    rm -f -- 2024-*.html 2024-*.json 2025-*.html 2025-*.json
    rm -f -- 2026-*.html 2026-*.json 2027-*.html 2027-*.json
  '
fi

echo
echo "Done. Current size:"
du -sh "$ROOT"
