/**
 * Unit smoke for CXSMILES star/index helpers.
 * Run: `npx tsx src/cxsmiles.smoke.ts`
 */
import {
  cxAtomLabels,
  smilesStarAtomIndices,
  starLabelsFromCxsmiles,
} from "./cxsmiles.js";

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(msg);
}

assert(
  JSON.stringify(cxAtomLabels("*C* |$R1;;;$|")) === JSON.stringify(["R1", null, null]),
  "cx first-only"
);
assert(
  JSON.stringify(cxAtomLabels("*C* |$;;R2;$|")) === JSON.stringify([null, null, "R2"]),
  "cx second-only"
);
assert(
  JSON.stringify(smilesStarAtomIndices("*C* |$;;R2;$|")) === JSON.stringify([0, 2]),
  "star indices *C*"
);
assert(
  JSON.stringify(starLabelsFromCxsmiles("*C* |$;;R2;$|")) ===
    JSON.stringify([null, "R2"]),
  "starLabels second-only encounter order"
);
assert(
  JSON.stringify(starLabelsFromCxsmiles("*C* |$R1;;;$|")) ===
    JSON.stringify(["R1", null]),
  "starLabels first-only encounter order"
);

console.log("cxsmiles smoke ok");
