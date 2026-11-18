#!/usr/bin/env bash
# Build the C ABI static library, link a C consumer against the header, run it.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release -p statementkit-c

out="$(mktemp -d)/smoke"
trap 'rm -rf "$(dirname "$out")"' EXIT

cc -std=c11 -Wall -Wextra -Werror \
   crates/statementkit-c/tests/smoke.c \
   -I crates/statementkit-c/include \
   target/release/libstatementkit_c.a \
   -lm -ldl -lpthread \
   -o "$out"

"$out"
