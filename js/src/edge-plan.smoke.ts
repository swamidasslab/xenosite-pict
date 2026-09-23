/**
 * EdgePlan host processor smoke (RDKit / MinimalLib only).
 * Schema / validate / plan_edge / CX chrome are covered in xpict-core.
 * Run: `npx tsx src/edge-plan.smoke.ts`
 */
import { initNative } from "./native.js";
import { buildAlignPlan, processEdgePlan } from "./edge-plan.js";

function assert(cond: unknown, msg: string): asserts cond {
  if (!cond) throw new Error(msg);
}

await initNative();

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
  assert(rows[1]!.ok && rows[1]!.method === "none", "child fallback ok");
  assert(rows[1]!.molecule != null, "fallback returns molecule");
}

console.log("edge-plan smoke ok");
