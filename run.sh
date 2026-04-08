#!/bin/bash
cd "$(dirname "$0")"
cargo install --path . --force --quiet 2>/dev/null
better-than-you "$@"
