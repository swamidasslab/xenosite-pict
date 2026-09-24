/**
 * Indigo WASM layout adapter stub.
 *
 * Primary browser layout path: indigo-ketcher / Indigo WASM builds.
 * Returns backend-agnostic coords only — SVG is drawn by ``draw/svg.ts``.
 */

/** Minimal input for the stub (structure fields only). */
export type IndigoMoleculeIn = {
  smiles?: string;
  cxsmiles?: string;
  esmiles?: string;
  molfile?: string;
};

/** Backend-agnostic 2D layout (coords only). */
export type IndigoMoleculeLayout = {
  id?: string;
  atoms: Array<{ index: number; element: string; x: number; y: number }>;
  bonds: Array<{ index: number; begin: number; end: number }>;
  backend: string;
  warnings?: string[];
};

export async function layoutWithIndigoWasm(
  mol: IndigoMoleculeIn
): Promise<IndigoMoleculeLayout> {
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
