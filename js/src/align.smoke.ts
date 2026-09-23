/**
 * Align smoke: RDKit MCS + generate_aligned_coords (BondCompare Any).
 * Symmetric cores may embed many ways — assert overlay hits, not fixed indices.
 * Run: `npx tsx src/align.smoke.ts`
 */
import { xpict } from "./api.js";

type Rendered = Awaited<ReturnType<typeof xpict.render>>;

function near(
  a: { x: number; y: number },
  b: { x: number; y: number },
  tol = 0.2
): boolean {
  return Math.hypot(a.x - b.x, a.y - b.y) < tol;
}

/** Count query atoms that land on some template atom (symmetric-equiv OK). */
function overlayHits(query: Rendered, template: Rendered, tol = 0.2): number {
  let hits = 0;
  for (const a of query.coords) {
    if (template.coords.some((b) => near(a, b, tol))) hits += 1;
  }
  return hits;
}

function assertHits(query: Rendered, template: Rendered, min: number, label: string) {
  const hits = overlayHits(query, template);
  if (hits < min) {
    throw new Error(`${label}: expected ≥${min} MCS overlay hits, got ${hits}`);
  }
}

function coordsKey(r: Rendered): string {
  return r.coords
    .map((a) => `${a.index}:${a.x.toFixed(4)},${a.y.toFixed(4)}`)
    .join("|");
}

// Asymmetric: para-F/Cl vs para-F/Br — unique MCS embedding for the F–ring–X axis.
{
  const tmpl = xpict.mol("Fc1ccc(Cl)cc1");
  const tR = await xpict.render(tmpl);
  const aligned = await xpict.render(xpict.mol("Fc1ccc(Br)cc1"), { align_to: tmpl });
  assertHits(aligned, tR, 7, "F-Cl → F-Br");
  // Attachment carbons + F should coincide (break ring symmetry).
  const fT = tR.coords.find((a) => a.element === "F");
  const fQ = aligned.coords.find((a) => a.element === "F");
  if (!fT || !fQ || !near(fT, fQ, 0.2)) {
    throw new Error("asymmetric: F atoms should coincide");
  }
}

// Ethyl ↔ pentyl (ring is symmetric; chain anchors the embedding — overlay hits).
{
  const ethyl = xpict.mol("c1ccccc1CC");
  const pentyl = xpict.mol("c1ccccc1CCCCC");
  const eR = await xpict.render(ethyl);
  assertHits(await xpict.render(pentyl, { align_to: ethyl }), eR, 8, "pentyl→ethyl");
  const pR = await xpict.render(pentyl);
  assertHits(await xpict.render(ethyl, { align_to: pentyl }), pR, 8, "ethyl→pentyl");
}

// Phenol ↔ benzoquinone (BondCompare Any; O breaks full ring symmetry).
{
  const phenol = xpict.mol("c1ccc(O)cc1");
  const quinone = xpict.mol("O=C1C=CC(=O)C=C1");
  const phR = await xpict.render(phenol);
  assertHits(await xpict.render(quinone, { align_to: phenol }), phR, 6, "quinone→phenol");
  const qR = await xpict.render(quinone);
  assertHits(await xpict.render(phenol, { align_to: quinone }), qR, 6, "phenol→quinone");
}

// Several queries on one template — template pose must not move.
{
  const tmpl = xpict.mol("c1ccc(O)cc1");
  const tR = await xpict.render(tmpl);
  const before = coordsKey(tR);
  const beforeFrame = tmpl.frame_molblock;
  if (!beforeFrame) throw new Error("template missing frame_molblock");

  for (const smi of ["O=C1C=CC(=O)C=C1", "c1ccccc1CC", "Fc1ccccc1O"]) {
    const aligned = await xpict.render(xpict.mol(smi), { align_to: tmpl });
    assertHits(aligned, tR, 6, `multi ${smi}`);
    if (tmpl.frame_molblock !== beforeFrame) {
      throw new Error(`template frame_molblock changed after aligning ${smi}`);
    }
    const tAgain = await xpict.render(tmpl);
    if (coordsKey(tAgain) !== before) {
      throw new Error(`template coords changed after aligning ${smi}`);
    }
  }
}

console.log("align smoke ok");
