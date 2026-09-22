/**
 * Indigo WASM layout adapter stub.
 *
 * Primary browser layout path: indigo-ketcher / Indigo WASM builds.
 * Returns backend-agnostic coords only — SVG is drawn by ``draw/svg.ts``.
 */

import type { MoleculeLayout, MoleculeSpec } from "../future/pict-types.js";

export async function layoutWithIndigoWasm(
  mol: MoleculeSpec
): Promise<MoleculeLayout> {
  const structure = mol.smiles ?? mol.cxsmiles ?? mol.esmiles ?? mol.molfile;
  if (!structure) {
    throw new Error("molecule needs smiles, cxsmiles, esmiles, or molfile");
  }
  // Placeholder until indigo-ketcher / Indigo WASM is wired in the browser build.
  throw new Error(
    `Indigo WASM layout not wired yet (got ${structure.slice(0, 32)}…). ` +
      "Use the Python package (epam.indigo) for layout today."
  );
}
