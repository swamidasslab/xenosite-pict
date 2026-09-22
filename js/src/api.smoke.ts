/**
 * Smoke: xpict.mol / xpict.render + scene → toSvg.
 * Run: `npx tsx src/api.smoke.ts` (after wasm build).
 */
import { xpict } from "./api.js";

if (typeof document !== "undefined") {
  throw new Error("api smoke expects Node (server-side) without document");
}

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

const starred = await xpict.render(xpict.mol("*C"), { star_labels: ["R1"] });
const starSvg = xpict.toSvg(starred.scene);
if (!starSvg.includes('data-text="R1"')) {
  throw new Error("star_labels should replace * with R1");
}
const bareStar = await xpict.render(xpict.mol("*C"), { star_labels: ["*"] });
const bareSvg = xpict.toSvg(bareStar.scene);
if (!bareSvg.includes('data-text="*"')) {
  throw new Error("bare * should keep data-text=*");
}
if (bareSvg.includes("<text")) {
  throw new Error("star label must be a glyph path, not <text>");
}

// CXSMILES trailer → star label without explicit star_labels (Python parity).
const fromCx = await xpict.render(xpict.mol("*C |$R1;$|"));
const fromCxSvg = xpict.toSvg(fromCx.scene);
if (!fromCxSvg.includes('data-text="R1"')) {
  throw new Error("CXSMILES |$R1;$| should auto-label * as R1");
}
const gsh = await xpict.render(xpict.mol("*C1C=C(O)C=CC1=O |$GSH;;;;;;;;$|"));
const gshSvg = xpict.toSvg(gsh.scene);
if (!gshSvg.includes('data-text="GSH"')) {
  throw new Error("CXSMILES GSH adduct should auto-label *");
}
// Explicit star_labels still wins over the trailer.
const override = await xpict.render(xpict.mol("*C |$R1;$|"), {
  star_labels: ["X"],
});
if (!xpict.toSvg(override.scene).includes('data-text="X"')) {
  throw new Error("explicit star_labels should override CX trailer");
}

// Multi-star: CX label slots must hit the right * by atom index.
function starLabelsByIndex(mol: Awaited<ReturnType<typeof xpict.render>>) {
  return mol.molecule.atoms
    .filter((a) => a.z === 0 || a.element === "*")
    .map((a) => ({ index: a.index, label: a.label ?? "*" }));
}

const firstOnly = await xpict.render(xpict.mol("*C* |$R1;;;$|"));
{
  const stars = starLabelsByIndex(firstOnly);
  if (stars.length !== 2) throw new Error(`expected 2 stars, got ${stars.length}`);
  if (stars[0]!.index !== 0 || stars[0]!.label !== "R1") {
    throw new Error(`first star should be R1 at 0, got ${JSON.stringify(stars)}`);
  }
  if (stars[1]!.index !== 2 || stars[1]!.label !== "*") {
    throw new Error(`second star should stay *, got ${JSON.stringify(stars)}`);
  }
  const texts = [...xpict.toSvg(firstOnly.scene).matchAll(/data-text="([^"]*)"/g)].map(
    (m) => m[1]
  );
  if (!texts.includes("R1") || !texts.includes("*")) {
    throw new Error(`first-only SVG labels ${JSON.stringify(texts)}`);
  }
}

const secondOnly = await xpict.render(xpict.mol("*C* |$;;R2;$|"));
{
  const stars = starLabelsByIndex(secondOnly);
  if (stars.length !== 2) throw new Error(`expected 2 stars, got ${stars.length}`);
  if (stars[0]!.label !== "*") {
    throw new Error(`first star should stay *, got ${JSON.stringify(stars)}`);
  }
  if (stars[1]!.index !== 2 || stars[1]!.label !== "R2") {
    throw new Error(`second star should be R2 at 2, got ${JSON.stringify(stars)}`);
  }
  const texts = [...xpict.toSvg(secondOnly.scene).matchAll(/data-text="([^"]*)"/g)].map(
    (m) => m[1]
  );
  if (!texts.includes("R2") || !texts.includes("*")) {
    throw new Error(`second-only SVG labels ${JSON.stringify(texts)}`);
  }
}

const both = await xpict.render(xpict.mol("*C* |$R1;;R2;$|"));
{
  const stars = starLabelsByIndex(both);
  if (stars.map((s) => s.label).join(",") !== "R1,R2") {
    throw new Error(`both stars labeled, got ${JSON.stringify(stars)}`);
  }
}

// Preferred document: nested PictSpec subset → Rendered[]
const batch = await xpict.depict({
  type: "group",
  children: [
    { type: "mol", smiles: "CCO", color: "#111" },
    { type: "mol", smiles: "CCCO" },
  ],
});
if (batch.length !== 2) throw new Error(`depict length ${batch.length}`);
if (batch[0]!.molecule.atoms.length !== 3) throw new Error("depict[0] atoms");
if (batch[1]!.molecule.atoms.length !== 4) throw new Error("depict[1] atoms");

const markush = await xpict.depict({
  type: "mol",
  smiles: "*c1ccccc1Cl",
  rgroups: ["$R_1$"],
});
{
  const texts = [...xpict.toSvg(markush[0]!.scene).matchAll(/data-text="([^"]*)"/g)].map(
    (m) => m[1]
  );
  if (!texts.includes("R₁")) {
    throw new Error(`rgroups $R_1$ should paint R₁, got ${JSON.stringify(texts)}`);
  }
}

console.log("api smoke ok", {
  svgBytes: svg.length,
  alignToMol: xpict.toSvg(alignedToMol.scene).length,
  alignToRendered: xpict.toSvg(alignedToRendered.scene).length,
  depictBatch: batch.length,
  serverSide: typeof document === "undefined",
});
