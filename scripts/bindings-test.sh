#!/usr/bin/env bash
# Build native + wasm libraries, then run every binding suite against the
# shared vectors in tests/vectors/. Assumes cargo, python3, go, node; Java
# uses a system JDK 22+ or the local download in bindings/java/.temurin.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release -p statementkit-c
cargo build --release -p statementkit-c --target wasm32-unknown-unknown

echo '== python (ctypes) =='
python3 bindings/python/test_parse_float.py
python3 bindings/python/test_stream.py

echo '== go (cgo) =='
(cd bindings/go && go test ./...)

echo '== node (raw wasm) =='
(cd bindings/node && node --test test/parse_float.test.js test/stream.test.js)

echo '== java (Panama FFM) =='
if command -v javac >/dev/null 2>&1 && \
   [ "$(javac -version 2>&1 | awk '{print $2}' | cut -d. -f1)" -ge 22 ]; then
  JC=javac; JA=java
else
  JC=bindings/java/.temurin/bin/javac; JA=bindings/java/.temurin/bin/java
fi
mkdir -p bindings/java/out
"$JC" -d bindings/java/out \
  bindings/java/src/org/statementkit/*.java \
  bindings/java/test/org/statementkit/*.java
STATEMENTKIT_LIB="$PWD/target/release/libstatementkit_c.so" \
  "$JA" --enable-native-access=ALL-UNNAMED -cp bindings/java/out \
  org.statementkit.StatementKitTest tests/vectors/parse_float.tsv

echo '== all bindings green =='
