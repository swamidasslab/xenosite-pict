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
  LayoutSpec,
  LegacyPictSpec,
  MarkSpec,
  MoleculeLayout,
  MoleculeSpec,
  MolNode,
  PictNode,
  PictSpec,
  RenderOptions,
  RingAttachmentSpec,
  RTableSpec,
  ShadeSpec,
} from "./types.js";

export { layoutWithIndigoWasm } from "./layout/indigo-wasm.js";
export { layoutToSvgStub } from "./draw/svg.js";

function isLegacyPict(spec: PictSpec): spec is import("./types.js").LegacyPictSpec {
  return (
    !!spec &&
    typeof spec === "object" &&
    "molecules" in spec &&
    Array.isArray((spec as import("./types.js").LegacyPictSpec).molecules) &&
    !("type" in spec)
  );
}

function assertPictSpec(spec: PictSpec): void {
  if (!spec || typeof spec !== "object") {
    throw new Error("PictSpec required");
  }
  if (isLegacyPict(spec)) {
    if (spec.molecules.length < 1) {
      throw new Error("PictSpec requires molecules: [{}]");
    }
    for (const m of spec.molecules) {
      if (!m.smiles && !m.cxsmiles && !m.esmiles && !m.molfile) {
        throw new Error("each molecule needs smiles, cxsmiles, esmiles, or molfile");
      }
    }
    return;
  }
  if (!("type" in spec) || typeof (spec as { type?: string }).type !== "string") {
    throw new Error("PictSpec requires type (nested) or molecules (legacy)");
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
  let mol: import("./types.js").MoleculeSpec | import("./types.js").MolNode;
  if (isLegacyPict(spec)) {
    mol = spec.molecules[0];
  } else if (spec.type === "mol") {
    mol = spec;
  } else {
    throw new Error("js stub: nested non-mol roots not wired yet; use legacy {molecules}");
  }
  const layout = await layoutWithIndigoWasm(mol);
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
