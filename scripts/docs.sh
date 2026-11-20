#!/usr/bin/env bash
# Build the mdBook site (docs/) into docs/book, or preview it live: docs.sh serve
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v mdbook >/dev/null; then
  echo "mdbook not found. Install with:" >&2
  echo "  cargo install mdbook --version 0.5.4 --locked" >&2
  exit 1
fi

if [[ "${1:-build}" == "serve" ]]; then
  exec mdbook serve docs --open --hostname 0.0.0.0 --port 30000
fi
mdbook build docs
echo "site written to docs/book (deployed from main via .github/workflows/docs.yml)"
