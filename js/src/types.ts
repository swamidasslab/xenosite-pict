/**
 * Shared PictSpec-shaped types (mirror schema/pict.schema.json).
 * Generated schema remains the source of truth for validation.
 */

export type OutputFormat = "svg" | "html";

export interface MoleculeSpec {
  id?: string;
  smiles?: string;
  cxsmiles?: string;
  /** MolParser E-SMILES (SMILES&lt;sep&gt;EXTENSION). */
  esmiles?: string;
  molfile?: string;
  title?: string;
  marks?: MarkSpec[];
  shade?: ShadeSpec;
  halo?: boolean;
  color?: string;
}

export interface MarkSpec {
  kind?: "atoms" | "bonds" | "substructure";
  atoms?: number[];
  bonds?: [number, number][];
  color?: string;
  label?: string;
}

export interface ShadeSpec {
  atoms?: number[];
  vmin?: number;
  vmax?: number;
}

export interface DiagramSpec {
  kind?: "row" | "grid" | "network" | "stack" | "single";
  columns?: number;
  /** Chemical MCS alignment before diagram placement (Python pure-Python MCS). */
  align?: boolean;
  elk_options?: Record<string, unknown>;
}

export interface PictSpec {
  molecules: MoleculeSpec[];
  diagram?: DiagramSpec;
  width?: number;
  height?: number;
}

/** Backend-agnostic 2D layout (coords only — drawing stays in this package). */
export interface AtomLayout {
  index: number;
  element: string;
  x: number;
  y: number;
  charge?: number;
  isotope?: number | null;
  /** Unpaired electron count (radical dots). */
  radical?: number;
  /** Display label; stars / R-groups put the name here (e.g. R1). */
  label?: string | null;
}

export interface BondLayout {
  index: number;
  begin: number;
  end: number;
  order?: number;
  stereo?: "up" | "down" | "either" | "none" | null;
}

export interface MoleculeLayout {
  id?: string;
  atoms: AtomLayout[];
  bonds: BondLayout[];
  backend: string;
  warnings?: string[];
}

export interface RenderOptions {
  backend?: string;
  format?: OutputFormat;
}
