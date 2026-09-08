/**
 * TypeScript types mirroring PictSpec (from schema/pict.schema.json).
 * Keep in sync with Python Pydantic contracts; schemas remain the source of truth.
 */

export type DiagramKind = "single" | "grid" | "network" | "reaction" | "html";
export type MarkKind = "atoms" | "bonds" | "substructure";
export type OutputFormat = "svg" | "html";

export interface MarkSpec {
  kind?: MarkKind;
  atoms?: number[] | null;
  bonds?: [number, number][] | null;
  color?: string | null;
  label?: string | null;
}

export interface ShadeSpec {
  atoms?: number[] | null;
  bonds?: number[] | null;
  colormap?: string;
  vmin?: number | null;
  vmax?: number | null;
}

export interface MoleculeSpec {
  id?: string | null;
  smiles?: string | null;
  cxsmiles?: string | null;
  esmiles?: string | null;
  molfile?: string | null;
  title?: string | null;
  marks?: MarkSpec[];
  shade?: ShadeSpec | null;
  halo?: boolean;
  color?: string | null;
}

export interface EdgeSpec {
  source: string;
  target: string;
  label?: string | null;
}

export interface DiagramSpec {
  kind?: DiagramKind;
  columns?: number | null;
  edges?: EdgeSpec[];
  elk_options?: Record<string, unknown>;
  align?: boolean;
}

/** Declarative depiction document (backend is runtime config, not part of this). */
export interface PictSpec {
  molecules: MoleculeSpec[];
  diagram?: DiagramSpec;
  width?: number | null;
  height?: number | null;
  meta?: Record<string, unknown>;
}

export interface RenderOptions {
  backend?: string;
  format?: OutputFormat;
}
