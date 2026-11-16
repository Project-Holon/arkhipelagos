#!/bin/sh
# The checks that need no cartridge (CI runs these on every push):
# crate tests, the generator's golden hashes (native and wasm32), finishable
# worlds, and the line checks that need no game data.
# tests/check.sh runs everything, with the cartridges.
set -e
cd "$(dirname "$0")/.."
T="${CARGO_TARGET_DIR:-target}"
mkdir -p out
cargo test --release
cargo build --release
cargo build --release -p arkh-gen-wasm --target wasm32-unknown-unknown
cargo build --release -p arkh-mod --target wasm32-unknown-unknown
fail=0
echo "== generator: golden hashes (native)"
while read -r h f; do
  got=$("$T/release/arkh-gen" hash "tests/configs/$f")
  if [ "$got" != "$h" ]; then echo "DIFFERS $f: $got, golden $h"; fail=1; fi
done < tests/golden.txt
echo "== generator: golden hashes (wasm32)"
node tests/wasm_hash.mjs "$T/wasm32-unknown-unknown/release/arkh_gen_wasm.wasm" tests/configs/*.json > out/wasm-hashes.txt
while read -r h f; do
  got=$(grep " tests/configs/$f\$" out/wasm-hashes.txt | cut -d' ' -f1)
  if [ "$got" != "$h" ]; then echo "DIFFERS (wasm32) $f: $got, golden $h"; fail=1; fi
done < tests/golden.txt
[ $fail = 0 ] && echo "golden hashes match, native and wasm32"
echo "== generator: finishable worlds"
"$T/release/arkh-gen" sweep 150
echo "== lines (no cartridge)"
"${PYTHON:-python3}" tools/checklines.py --no-carts
exit $fail
