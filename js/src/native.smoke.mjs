/** Smoke test — run: node js/src/native.smoke.mjs (after wasm build). */
import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import init, {
  multiBondOffset,
  plotdotRings,
  offsetPx,
  capsuleHaloPathD,
  coreVersion,
} from "./wasm/xpict_core.js";

const here = dirname(fileURLToPath(import.meta.url));
await init({
  module_or_path: await readFile(join(here, "wasm", "xpict_core_bg.wasm")),
});
const off = multiBondOffset(20);
if (Math.abs(off - 3) > 1e-9) throw new Error(`offset ${off}`);
const rings = plotdotRings(1.0, 4);
if (rings.length !== 8) throw new Error(`flat rings len ${rings.length}`);
if (Math.abs(offsetPx() - 3) > 1e-9) throw new Error("OFFSET_PX");
const d = capsuleHaloPathD(0, 0, 20, 0, 1, 2);
if (!d || !d.endsWith("Z")) throw new Error("capsule path");
console.log("native smoke ok", {
  off,
  rings: rings.length / 2,
  ver: coreVersion(),
  d: d.length,
});
