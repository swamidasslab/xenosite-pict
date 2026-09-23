/**
 * Align smoke: RDKit MCS (element + hybridization; BondCompare Any);
 * Depictor/MinimalLib align with atom matches only.
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

// Phenol ↔ benzoquinone (O/SP2 anchors substituent).
{
  const phenol = xpict.mol("c1ccc(O)cc1");
  const quinone = xpict.mol("O=C1C=CC(=O)C=C1");
  const phR = await xpict.render(phenol);
  assertHits(await xpict.render(quinone, { align_to: phenol }), phR, 6, "quinone→phenol");
  const qR = await xpict.render(quinone);
  assertHits(await xpict.render(phenol, { align_to: quinone }), qR, 6, "phenol→quinone");
}

// Aniline ↔ quinone imine (N/SP2 anchors; N≠O so not benzoquinone).
{
  const aniline = xpict.mol("Nc1ccccc1");
  const imine = xpict.mol("O=C1C=CC(=N)C=C1");
  const aR = await xpict.render(aniline);
  assertHits(await xpict.render(imine, { align_to: aniline }), aR, 6, "imine→aniline");
}

// Aliphatic cyclohexane ether must NOT snap onto benzoquinone (SP3≠SP2).
{
  const quinone = xpict.mol("O=C1C=CC(=O)C=C1");
  const qR = await xpict.render(quinone);
  const chain = xpict.mol("C1CCCCC1CCOCCCCCC");
  const free = await xpict.render(chain);
  const aligned = await xpict.render(xpict.mol("C1CCCCC1CCOCCCCCC"), {
    align_to: quinone,
  });
  const hits = overlayHits(aligned, qR);
  if (hits >= 4) {
    throw new Error(`aliphatic→quinone should not MCS-align (hits=${hits})`);
  }
  // Free layout coords preserved (no template snap).
  if (coordsKey(aligned) !== coordsKey(free)) {
    throw new Error("aliphatic→quinone changed coords despite MCS reject");
  }
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
