#!/bin/sh
# Every check of the arkhipelagos tools. Needs the 12 cartridges' data
# (tools/carts.local.json) for the script check, node for the wasm check.
set -e
cd "$(dirname "$0")/.."
T="${CARGO_TARGET_DIR:-target}"
cargo build --release
cargo build --release -p arkh-gen-wasm --target wasm32-unknown-unknown
cargo build --release -p arkh-mod --target wasm32-unknown-unknown
cargo test --release
echo "== field scripts, 12 cartridges"
python tools/check_scripts.py
echo "== apply step: written archives read back, 12 cartridges"
python tools/check_apply.py
echo "== mod: solo flow applied to an English and a Japanese cartridge"
for code in IRBO IRAJ; do
  data=$(python -I -c "import json,sys; print(json.load(open('tools/carts.local.json'))[sys.argv[1]]['data'])" $code)
  ARKH_CART_CODE=$code ARKH_CART_DATA="$data" cargo test --release -q -p arkh-mod --test solo
done
echo "== mod: a two-player online world against metaxy (ARKH_METAXY=host:port)"
if [ -n "${ARKH_METAXY:-}" ]; then cargo test --release -q -p arkh-mod --test online
else echo "ARKH_METAXY not set: skipped"; fi
echo "== mod: wasm imports against eidos's v6 header"
H="${EIDOS_SDK:-}/api/v6/eidos_wasm.h"
if [ -f "$H" ]; then python -I tools/wasm_imports.py "$T/wasm32-unknown-unknown/release/arkh_mod.wasm" "$H" | tail -1
else echo "no eidos header (set EIDOS_SDK): skipped"; fi
echo "== generator: golden hashes (native)"
fail=0
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
"$T/release/arkh-gen" sweep 200
exit $fail
