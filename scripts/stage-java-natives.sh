#!/usr/bin/env bash
# Cross-compile the C ABI cdylib for the jar's three platforms and stage
# it under bindings/java/native/lib/<os>-<arch>/ for Maven packaging.
# Requires: cargo-zigbuild, ziglang (pip), the three rust std targets.
set -euo pipefail
export ZIG="${ZIG:-$(python3 -c 'import os, ziglang; print(os.path.join(os.path.dirname(ziglang.__file__), "zig"))')}"
cd "$(dirname "$0")/.."
# The .2.28 glibc pin matches the wheels' manylinux_2_28 baseline.
cargo zigbuild --release -p statementkit-c --target x86_64-unknown-linux-gnu.2.28 --locked
cargo zigbuild --release -p statementkit-c --target aarch64-unknown-linux-gnu.2.28 --locked
cargo zigbuild --release -p statementkit-c --target x86_64-pc-windows-gnu --locked
stage() {
    mkdir -p "bindings/java/native/lib/$2"
    cp "target/$1/release/$3" "bindings/java/native/lib/$2/$3"
}
stage x86_64-unknown-linux-gnu linux-x86_64 libstatementkit_c.so
stage aarch64-unknown-linux-gnu linux-aarch64 libstatementkit_c.so
stage x86_64-pc-windows-gnu windows-x86_64 statementkit_c.dll
