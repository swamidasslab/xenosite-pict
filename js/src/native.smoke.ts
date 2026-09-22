/** Smoke test — run: `npm run test:native` (after wasm build). */
import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  capsuleHaloPathD,
  coreVersion,
  depictMolecule,
  initNative,
  multiBondOffset,
  offsetPx,
  plotdotRings,
} from "./native.js";

const here = dirname(fileURLToPath(import.meta.url));
await initNative({
  module_or_path: await readFile(join(here, "wasm", "xpict_core_bg.wasm")),
});

const off = multiBondOffset(20);
if (Math.abs(off - 3) > 1e-9) throw new Error(`offset ${off}`);

const rings = plotdotRings(1.0, 4);
if (rings.length !== 4) throw new Error(`rings len ${rings.length}`);
if (Math.abs(offsetPx() - 3) > 1e-9) throw new Error("OFFSET_PX");

const d = capsuleHaloPathD(0, 0, 20, 0, 1, 2);
if (!d || !d.endsWith("Z")) throw new Error("capsule path");

const sceneJson = depictMolecule(
  JSON.stringify({
    id: "etoh",
    atoms: [
      { index: 0, element: "C", x: 0, y: 0 },
      { index: 1, element: "C", x: 20, y: 0 },
      { index: 2, element: "O", x: 30, y: 10, label: "OH" },
    ],
    bonds: [
      { index: 0, begin: 0, end: 1, order: 1 },
      { index: 1, begin: 1, end: 2, order: 1 },
    ],
  })
);
const scene = JSON.parse(sceneJson) as {
  viewports: Array<{ layers: Array<{ name: string; primitives: unknown[] }> }>;
};
const bondLayer = scene.viewports[0]?.layers.find((l) => l.name === "bonds");
if (!bondLayer || bondLayer.primitives.length < 2) {
  throw new Error("depictMolecule missing bond primitives");
}

console.log("native smoke ok", {
  off,
  rings: rings.length,
  ver: coreVersion(),
  d: d.length,
  bonds: bondLayer.primitives.length,
});
