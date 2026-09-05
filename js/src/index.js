/**
 * JS engine stub for xenosite.pict.
 * Validates PictSpec-shaped objects and leaves layout/draw to WASM backends (future).
 *
 * Shared contracts: ../schema/*.schema.json (generated from Pydantic).
 */

/**
 * @param {object} spec — PictSpec-like JSON
 * @param {{ backend?: string, format?: 'svg'|'html' }} [options]
 * @returns {Promise<string>}
 */
export async function render(spec, options = {}) {
  if (!spec || !Array.isArray(spec.molecules) || spec.molecules.length < 1) {
    throw new Error("PictSpec requires molecules: [{}]");
  }
  for (const m of spec.molecules) {
    if (!m.smiles && !m.cxsmiles && !m.esmiles && !m.molfile) {
      throw new Error("each molecule needs smiles, cxsmiles, esmiles, or molfile");
    }
  }
  const backend = options.backend ?? "indigo";
  const format = options.format ?? "svg";
  // Placeholder until Indigo WASM + scene drawer are wired.
  throw new Error(
    `js render not implemented yet (backend=${backend}, format=${format}). ` +
      "Use the Python package for now; this stub validates the shared contract."
  );
}

export class Pict {
  constructor(options = {}) {
    this.backend = options.backend ?? "indigo";
    this.format = options.format ?? "svg";
  }
  render(spec) {
    return render(spec, { backend: this.backend, format: this.format });
  }
}
