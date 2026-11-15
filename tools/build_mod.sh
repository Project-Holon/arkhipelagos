#!/bin/sh
# Build the eidos mod's code into the mod folder, before katalogos packs it
# (the release workflow's build step). Runs from the repository root.
set -e
if [ ! -f mod/eidos_mod.txt ]; then
  echo "There is no mod folder yet (mod/eidos_mod.txt): nothing to release." >&2
  exit 1
fi
rustup target add wasm32-unknown-unknown
cargo build --release -p arkh-mod --target wasm32-unknown-unknown
cp "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/arkh_mod.wasm" mod/arkhipelagos.wasm
