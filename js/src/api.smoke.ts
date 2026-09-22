/**
 * Smoke: present a template, align, draw → SVG + atom coords matching ink.
 * Run: `npx tsx src/api.smoke.ts` (after wasm build).
 */
import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  clearTemplate,
  currentTemplate,
  draw,
  init,
  layout,
  layoutPresentedTemplate,
  presentTemplate,
} from "./api.js";

const here = dirname(fileURLToPath(import.meta.url));

// Server-side path: no DOM — RDKit via npm, wasm via bytes.
if (typeof document !== "undefined") {
  throw new Error("api smoke expects Node (server-side) without document");
}

await init({
  wasm: {
    module_or_path: await readFile(join(here, "wasm", "xpict_core_bg.wasm")),
  },
});

presentTemplate("c1ccccc1");
if (currentTemplate() !== "c1ccccc1") {
  throw new Error("presentTemplate did not stick");
}

const benzene = await layoutPresentedTemplate();
const toluene = await layout("Cc1ccccc1");

for (let i = 1; i <= 6; i++) {
  const a = toluene.atoms[i]!;
  const hit = benzene.atoms.some(
    (b) => Math.hypot(a.x - b.x, a.y - b.y) < 0.05
  );
  if (!hit) {
    throw new Error(`toluene atom ${i} (${a.x},${a.y}) not on benzene template`);
  }
}
const methyl = toluene.atoms[0]!;
if (benzene.atoms.some((b) => Math.hypot(methyl.x - b.x, methyl.y - b.y) < 0.05)) {
  throw new Error("methyl should not sit on the ring template");
}

const drawn = await draw("Oc1ccccc1", { mark_atoms: [0] });
if (!drawn.svg.includes("<svg") || !drawn.svg.includes("viewBox")) {
  throw new Error("draw missing svg root");
}
if (Math.abs(drawn.width - drawn.scene.width) > 1e-9) {
  throw new Error("width mismatch");
}

// Marked atom circle in the scene must sit on the returned atom coord.
const marks = drawn.scene.viewports[0]?.layers.find((l) => l.name === "marks");
const circle = marks?.primitives.find(
  (p) => p.kind === "circle"
) as { kind: "circle"; cx: number; cy: number } | undefined;
if (!circle) throw new Error("expected mark circle");
const oAtom = drawn.atoms.find((a) => a.index === 0);
if (!oAtom) throw new Error("missing O atom");
if (Math.hypot(oAtom.x - circle.cx, oAtom.y - circle.cy) > 1e-6) {
  throw new Error(
    `atom SVG coord (${oAtom.x},${oAtom.y}) != mark (${circle.cx},${circle.cy})`
  );
}
if (!drawn.imgDataUri.startsWith("data:image/svg+xml")) {
  throw new Error("missing img data URI");
}

clearTemplate();
const free = await layout("Cc1ccccc1");
if (free.atoms.length !== 7) throw new Error("free layout atom count");

console.log("api smoke ok", {
  templateCleared: currentTemplate() === null,
  tolueneAtoms: toluene.atoms.length,
  svgBytes: drawn.svg.length,
  atomsMatchMarks: true,
  serverSide: typeof document === "undefined",
});
