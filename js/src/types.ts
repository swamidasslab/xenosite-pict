/**
 * Shared PictSpec-shaped types (mirror schema/pict.schema.json).
 * Generated schema remains the source of truth for validation.
 */

export type OutputFormat = "svg" | "html";

/** Molecule caption position relative to the drawing. */
export type LabelPos = "bottom" | "top" | "left" | "right";

/**
 * Molecule caption. Shorthand:
 * - `"ethanol"` → defaults (`pos: "bottom"`)
 * - `{ text: "ethanol" }` → same
 * - `{ text: "ethanol", pos: "top" }` → override position
 *
 * Legacy `title` is accepted as an alias for `label`.
 */
export type LabelSpec =
  | string
  | {
      text: string;
      pos?: LabelPos;
    };

export interface MoleculeSpec {
  id?: string;
  smiles?: string;
  cxsmiles?: string;
  /** MolParser E-SMILES (SMILES&lt;sep&gt;EXTENSION). */
  esmiles?: string;
  molfile?: string;
  /** Molecule caption (string shorthand or expanded object). */
  label?: LabelSpec | null;
  /** @deprecated Use `label`. */
  title?: LabelSpec | null;
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

export interface EdgeSpec {
  source: string;
  target: string;
  label?: string;
  /** Semantic role (enzyme, inhibits, …) — not drawn by default. */
  role?: string;
  /** Arrow head/shaft style. */
  arrow?: "forward" | "equilibrium" | "open" | "line";
  color?: string;
  stroke_width?: number;
  dashed?: boolean;
}

export interface DiagramSpec {
  kind?: "single" | "grid" | "network" | "reaction" | "html";
  columns?: number;
  edges?: EdgeSpec[];
  /** Align later molecules onto the first. RDKit template depiction when installed, else rigid rotate/translate. */
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
