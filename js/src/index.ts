/**
 * JS/TS engine for xpict.
 *
 * Validates PictSpec-shaped objects; layout via Indigo WASM (stub);
 * SVG via shared scene rules (stub). Shared contracts: ``../schema/*.schema.json``.
 */

import { layoutToSvgStub } from "./draw/svg.js";
import { layoutWithIndigoWasm } from "./layout/indigo-wasm.js";
import type { PictSpec, RenderOptions } from "./types.js";

export type {
  AnnotationSpec,
  AnnotKind,
  AnnotPrefer,
  AtomLayout,
  BondLayout,
  DiagramSpec,
  MarkSpec,
  MoleculeLayout,
  MoleculeSpec,
  PictSpec,
  RenderOptions,
  ShadeSpec,
} from "./types.js";

export { layoutWithIndigoWasm } from "./layout/indigo-wasm.js";
export { layoutToSvgStub } from "./draw/svg.js";

function assertPictSpec(spec: PictSpec): void {
  if (!spec || !Array.isArray(spec.molecules) || spec.molecules.length < 1) {
    throw new Error("PictSpec requires molecules: [{}]");
  }
  for (const m of spec.molecules) {
    if (!m.smiles && !m.cxsmiles && !m.esmiles && !m.molfile) {
      throw new Error("each molecule needs smiles, cxsmiles, esmiles, or molfile");
    }
  }
}

/**
 * Render a PictSpec to SVG/HTML.
 * Validates the shared contract; throws until Indigo WASM + drawer are wired.
 */
export async function render(
  spec: PictSpec,
  options: RenderOptions = {}
): Promise<string> {
  assertPictSpec(spec);
  const backend = options.backend ?? "indigo";
  const format = options.format ?? "svg";

  if (backend !== "indigo") {
    throw new Error(
      `js backend=${backend} not supported yet (indigo WASM only in scaffold)`
    );
  }

  // Exercise the adapter stubs so the call graph matches Python.
  const layout = await layoutWithIndigoWasm(spec.molecules[0]);
  const svg = layoutToSvgStub(layout);
  if (format === "html") {
    return `<!DOCTYPE html><html><body>${svg}</body></html>`;
  }
  return svg;
}

export class Pict {
  backend: string;
  format: "svg" | "html";

  constructor(options: RenderOptions = {}) {
    this.backend = options.backend ?? "indigo";
    this.format = options.format ?? "svg";
  }

  render(spec: PictSpec): Promise<string> {
    return render(spec, { backend: this.backend, format: this.format });
  }
}
