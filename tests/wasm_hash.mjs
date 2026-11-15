// Run the wasm32 build of the generator on configuration files and print
// each world's hash, to compare with the native `arkh-gen hash`.
// usage: node tests/wasm_hash.mjs <arkh_gen_wasm.wasm> <config.json>...
import { readFileSync } from "node:fs";

const [wasmPath, ...configs] = process.argv.slice(2);
const { instance } = await WebAssembly.instantiate(readFileSync(wasmPath), {});
const { memory, arkh_alloc, arkh_world_hash } = instance.exports;
for (const c of configs) {
  const bytes = readFileSync(c);
  const ptr = arkh_alloc(bytes.length);
  new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
  const h = arkh_world_hash(ptr, bytes.length);
  console.log(`${BigInt.asUintN(64, h).toString(16).padStart(16, "0")}  ${c}`);
}
