/**
 * Smoke: xpict.mol / xpict.render + scene → toSvg.
 * Run: `npx tsx src/api.smoke.ts` (after wasm build).
 */
import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { xpict } from "./api.js";

const here = dirname(fileURLToPath(import.meta.url));

if (typeof document !== "undefined") {
  throw new Error("api smoke expects Node (server-side) without document");
}

await xpict.init({
  wasm: {
    module_or_path: await readFile(join(here, "wasm", "xpict_core_bg.wasm")),
  },
});

const mol = xpict.mol("c1ccccc1");
const rendered = await xpict.render(mol);
if (!rendered.scene?.viewports?.length) throw new Error("missing scene");
if (rendered.coords.length !== 6) throw new Error("coords length");
if (rendered.svg_coords.length !== 6) throw new Error("svg_coords length");
if (!mol.frame_molblock) throw new Error("mol should carry frame_molblock");
if (!rendered.frame_molblock) throw new Error("rendered should carry frame");

const svg = xpict.toSvg(rendered.scene);
if (!svg.includes("<svg") || !svg.includes("viewBox")) {
  throw new Error("toSvg missing root");
}

// Tweak scene then re-serialize.
rendered.scene.width = Math.ceil(rendered.scene.width);
const tweaked = xpict.toSvg(rendered.scene);
if (!tweaked.includes(`width="${rendered.scene.width}"`)) {
  throw new Error("tweaked scene not reflected in toSvg");
}

const dx = rendered.svg_coords[0]!.x - rendered.coords[0]!.x;
const dy = rendered.svg_coords[0]!.y - rendered.coords[0]!.y;
for (let i = 0; i < 6; i++) {
  const a = rendered.coords[i]!;
  const s = rendered.svg_coords[i]!;
  if (Math.hypot(s.x - (a.x + dx), s.y - (a.y + dy)) > 1e-6) {
    throw new Error(`svg_coords/coords mismatch at ${i}`);
  }
}

const next = xpict.mol("Cc1ccccc1");
const alignedToMol = await xpict.render(next, { align_to: mol, mark_atoms: [0] });
for (let i = 1; i <= 6; i++) {
  const a = alignedToMol.coords[i]!;
  const hit = rendered.coords.some(
    (b) => Math.hypot(a.x - b.x, a.y - b.y) < 0.05
  );
  if (!hit) throw new Error(`align_to mol: atom ${i} not on template`);
}

const alignedToRendered = await xpict.render(xpict.mol("Oc1ccccc1"), {
  align_to: rendered,
});
for (let i = 1; i <= 6; i++) {
  const a = alignedToRendered.coords[i]!;
  const hit = rendered.coords.some(
    (b) => Math.hypot(a.x - b.x, a.y - b.y) < 0.05
  );
  if (!hit) throw new Error(`align_to rendered: atom ${i} not on template`);
}

const marks = alignedToMol.scene.viewports[0]?.layers.find((l) => l.name === "marks");
const circle = marks?.primitives.find((p) => p.kind === "circle") as
  | { kind: "circle"; cx: number; cy: number }
  | undefined;
if (!circle) throw new Error("expected mark circle");
const methyl = alignedToMol.svg_coords.find((a) => a.index === 0);
if (!methyl) throw new Error("missing methyl");
if (Math.hypot(methyl.x - circle.cx, methyl.y - circle.cy) > 1e-6) {
  throw new Error("svg_coords must match mark position in scene");
}

const uri = xpict.toImgDataUri(alignedToMol.scene);
if (!uri.startsWith("data:image/svg+xml")) throw new Error("bad data uri");

console.log("api smoke ok", {
  svgBytes: svg.length,
  alignToMol: xpict.toSvg(alignedToMol.scene).length,
  alignToRendered: xpict.toSvg(alignedToRendered.scene).length,
  serverSide: typeof document === "undefined",
});
