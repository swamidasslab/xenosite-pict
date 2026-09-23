/**
 * EdgePlan processor smoke (parity with Python tests/test_edge_plan.py).
 * Run: `npx tsx src/edge-plan.smoke.ts`
 */
import {
  buildAlignPlan,
  processEdgePlan,
  validateEdgePlan,
} from "./edge-plan.js";

function assert(cond: unknown, msg: string): asserts cond {
  if (!cond) throw new Error(msg);
}

{
  const bad = buildAlignPlan({
    templateSource: "CCO",
    querySource: "CCCO",
  });
  bad.tasks[0]!.roots[0]!.template_for![0]!.id = "m_0";
  let threw = false;
  try {
    validateEdgePlan(bad);
  } catch {
    threw = true;
  }
  assert(threw, "duplicate ids should fail validate");
}

{
  const plan = buildAlignPlan({
    templateSource: "c1ccccc1",
    querySource: "Cc1ccccc1",
    atomMap: [
      [1, 0],
      [2, 1],
      [3, 2],
      [4, 3],
      [5, 4],
      [6, 5],
    ],
  });
  const result = await processEdgePlan(plan);
  const rows = result.results[0]!.molecules;
  assert(rows.length === 2, "two molecules");
  assert(rows[0]!.ok && rows[0]!.method === "free", "root free");
  assert(rows[1]!.ok && rows[1]!.method === "atom_map", "child atom_map");
  assert(rows[1]!.molecule?.atoms.length === 7, "toluene atoms");
}

{
  const plan = buildAlignPlan({
    templateSource: "c1ccc(O)cc1",
    querySource: "O=C1C=CC(=O)C=C1",
  });
  const result = await processEdgePlan(plan);
  const child = result.results[0]!.molecules[1]!;
  assert(child.ok && child.method === "mcs", `expected mcs, got ${child.method}`);
}

{
  const plan = buildAlignPlan({
    templateSource: "O=C1C=CC(=O)C=C1",
    querySource: "C1CCCCC1CCOCCCCCC",
  });
  const result = await processEdgePlan(plan);
  const rows = result.results[0]!.molecules;
  assert(rows[0]!.ok, "root ok");
  assert(rows[1]!.ok && rows[1]!.method === "none", "child fallback ok");
  assert(rows[1]!.molecule != null, "fallback returns molecule");
  assert(result.results[0]!.ok, "task ok when all have coords");
}

console.log("edge-plan smoke ok");
