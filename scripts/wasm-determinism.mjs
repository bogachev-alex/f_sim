// Сверка хэшей в WASM с нативными:
// node wasm-determinism.mjs <pkg-dir> <seed> <ticks> <sandbox-hash> <scenario> <scenario-hash> <match-hash>
import { createRequire } from "node:module";
import path from "node:path";

const [pkg, seed, ticks, sandbox, scenario, scenarioHash, matchHash] = process.argv.slice(2);
const require = createRequire(import.meta.url);
const wasm = require(path.resolve(pkg, "fsim_wasm.js"));
const checks = [
  ["песочница", wasm.determinism_hash(BigInt(seed), Number(ticks)), sandbox],
  [`сценарий ${scenario}`, wasm.scenario_hash(BigInt(seed), Number(ticks), scenario), scenarioHash],
  ["матч", wasm.match_hash(BigInt(seed), Number(ticks), "positional", "gegenpress"), matchHash],
];
let ok = true;
for (const [name, got, native] of checks) {
  console.log(`${name}: wasm ${got}, native ${native}`);
  if (got !== native) ok = false;
}
if (!ok) {
  console.error("НЕСОВПАДЕНИЕ натив и WASM");
  process.exit(1);
}
console.log("совпадает");
