/**
 * Paint every review case through JS/WASM. Run: `npx tsx review/catalog.smoke.ts`
 */
import { loadCasesFromDisk } from "./catalog/fromDisk.js";
import { xpict } from "../src/api.js";

if (typeof document !== "undefined") {
  throw new Error("catalog smoke expects Node");
}

const cases = await loadCasesFromDisk();
if (!cases.length) throw new Error("no review cases under js/review/cases/");

let failed = 0;
for (const c of cases) {
  try {
    const rows = await xpict.depict(c.spec);
    if (!rows.length) throw new Error("no rows");
    const svg = rows.map((r) => xpict.toSvg(r.scene)).join("");
    if (!svg.includes("<svg")) throw new Error("toSvg missing <svg>");
    console.log(`ok ${c.group}/${c.id}`);
  } catch (err) {
    failed += 1;
    const msg = err instanceof Error ? err.message : String(err);
    console.error(`FAIL ${c.file}: ${msg}`);
  }
}

if (failed) {
  throw new Error(`${failed}/${cases.length} review cases failed to paint`);
}
console.log(`catalog smoke: ${cases.length} cases`);
