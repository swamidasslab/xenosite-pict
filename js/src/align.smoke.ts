/**
 * Align smoke: MCS Kabsch (aromatic ↔ aliphatic / quinone).
 * Run: `npx tsx src/align.smoke.ts`
 */
import { xpict } from "./api.js";

function near(
  a: { x: number; y: number },
  b: { x: number; y: number },
  tol = 0.15
): boolean {
  return Math.hypot(a.x - b.x, a.y - b.y) < tol;
}

/** Every mapped query atom must land on some template atom (MCS overlay). */
function assertOverlay(
  query: Awaited<ReturnType<typeof xpict.render>>,
  template: Awaited<ReturnType<typeof xpict.render>>,
  queryIdxs: number[],
  label: string
) {
  for (const i of queryIdxs) {
    const a = query.coords[i];
    if (!a) throw new Error(`${label}: missing query atom ${i}`);
    const hit = template.coords.some((b) => near(a, b));
    if (!hit) {
      throw new Error(
        `${label}: query atom ${i} @ (${a.x.toFixed(2)},${a.y.toFixed(2)}) not on template`
      );
    }
  }
}

const benzene = xpict.mol("c1ccccc1");
const benzR = await xpict.render(benzene);

// Toluene onto benzene (subset / MCS ring)
{
  const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), { align_to: benzene });
  assertOverlay(aligned, benzR, [1, 2, 3, 4, 5, 6], "toluene→benzene");
}

// Ethyl ↔ pentyl both directions (shared Ph-CH2-CH3 core)
{
  const ethyl = xpict.mol("c1ccccc1CC");
  const pentyl = xpict.mol("c1ccccc1CCCCC");
  const eR = await xpict.render(ethyl);
  const pOnE = await xpict.render(pentyl, { align_to: ethyl });
  assertOverlay(pOnE, eR, [0, 1, 2, 3, 4, 5, 6, 7], "pentyl→ethyl");

  const pR = await xpict.render(pentyl);
  const eOnP = await xpict.render(ethyl, { align_to: pentyl });
  assertOverlay(eOnP, pR, [0, 1, 2, 3, 4, 5, 6, 7], "ethyl→pentyl");
}

// Demo pair: aliphatic cyclohexane ↔ aromatic benzene (+ star)
{
  const cyclo = xpict.mol("*C1CCCCC1");
  const arom = xpict.mol("*c1ccccc1");
  const cR = await xpict.render(cyclo);
  const aOnC = await xpict.render(arom, { align_to: cyclo });
  // All 7 heavy atoms of aromatic should overlay cyclo MCS
  assertOverlay(aOnC, cR, [0, 1, 2, 3, 4, 5, 6], "benzene→cyclohexane");

  const aR = await xpict.render(arom);
  const cOnA = await xpict.render(cyclo, { align_to: arom });
  assertOverlay(cOnA, aR, [0, 1, 2, 3, 4, 5, 6], "cyclohexane→benzene");
}

// Benzoquinone ↔ phenol (BondCompare Any)
{
  const phenol = xpict.mol("c1ccc(O)cc1");
  const quinone = xpict.mol("O=C1C=CC(=O)C=C1");
  const phR = await xpict.render(phenol);
  const qOnPh = await xpict.render(quinone, { align_to: phenol });
  // 7-atom MCS: ring + one O — quinone atoms that are in MCS should hit phenol
  let hits = 0;
  for (const a of qOnPh.coords) {
    if (phR.coords.some((b) => near(a, b))) hits += 1;
  }
  if (hits < 6) {
    throw new Error(`quinone→phenol: expected ≥6 MCS hits, got ${hits}`);
  }

  const qR = await xpict.render(quinone);
  const phOnQ = await xpict.render(phenol, { align_to: quinone });
  hits = 0;
  for (const a of phOnQ.coords) {
    if (qR.coords.some((b) => near(a, b))) hits += 1;
  }
  if (hits < 6) {
    throw new Error(`phenol→quinone: expected ≥6 MCS hits, got ${hits}`);
  }
}

console.log("align smoke ok");
