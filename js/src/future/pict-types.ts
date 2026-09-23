/**
 * Future nested PictSpec types (design / refinement — not the ship surface).
 *
 * Public API types are in ``api.ts`` (``Mol`` / ``MolSpec`` /
 * ``DepictSpec`` / ``Rendered``). This file mirrors
 * ``schema/future/xpict.schema.json``.
 *
 * Canonical form is a nested node tree (`type` + `children` + `layout`).
 * Legacy flat `{ molecules, diagram }` remains for the Python lab path.
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
 * Text supports light markup (not full TeX): `\alpha` → α, `**bold**` /
 * `*italic*` or `\textbf{}` / `\textit{}`. Optional `$…$` is stripped.
 *
 * Legacy `title` is accepted as an alias for `label`.
 */
export type LabelSpec =
  | string
  | {
      text: string;
      pos?: LabelPos;
    };

/** Hierarchical layout knobs on any node. */
export interface LayoutSpec {
  columns?: number;
  direction?: "row" | "column";
  gap?: number;
  attach?: "below" | "above" | "left" | "right";
  width?: number;
  height?: number;
  halo?: boolean;
  align?: boolean;
  elk_options?: Record<string, unknown>;
  output?: "root" | "children" | "leaves" | "all";
  label?: LabelSpec | null;
  caption?: string;
  above?: unknown[];
  below?: unknown[];
  edges?: EdgeSpec[];
}

export type NodeType =
  | "mol"
  | "arrow"
  | "text"
  | "image"
  | "table"
  | "ref"
  | "annotation"
  | "group"
  | "grid"
  | "stack"
  | "reaction"
  | "network";

export interface NodeBase {
  type: NodeType;
  id?: string;
  panel?: string;
  layout?: LayoutSpec;
  meta?: Record<string, unknown>;
  children?: PictNode[];
}

export interface MolNode extends NodeBase {
  type: "mol";
  smiles?: string;
  cxsmiles?: string;
  esmiles?: string;
  molfile?: string;
  ids?: Record<string, number | number[]>;
  rings?: Record<string, number[]>;
  rgroups?: (string | null)[] | Record<string, string | null>;
  /**
   * Labels for `*` atoms in layout encounter order (chem markup OK).
   * Wins over `rgroups` / CX aliases when set. Prefer on the public document API.
   */
  star_labels?: (string | null)[];
  ring_attachments?: RingAttachmentSpec[];
  rtable?: string[][] | RTableSpec;
  marks?: MarkSpec[];
  annotations?: AnnotationSpec[];
  shade?: ShadeSpec;
  /** Ink color for backbone bonds and atom labels (CSS). Default `#111`. */
  color?: string;
}

export interface ArrowNode extends NodeBase {
  type: "arrow";
  arrow?: "forward" | "equilibrium" | "open" | "line";
  label?: string;
  role?: string;
  color?: string;
  stroke_width?: number;
  dashed?: boolean;
}

export interface TextNode extends NodeBase {
  type: "text";
  text: string;
}

export interface ImageNode extends NodeBase {
  type: "image";
  src: string;
  alt?: string;
}

export interface TableNode extends NodeBase {
  type: "table";
  columns?: string[];
  rows?: unknown[][];
}

export interface RefNode extends NodeBase {
  type: "ref";
  ref: string;
}

export interface AnnotationNode extends NodeBase {
  type: "annotation";
  kind?: AnnotKind;
  atoms?: number[];
  bonds?: [number, number][];
  ring?: number[];
  label?: string;
  color?: string;
  arrow?: boolean;
  prefer?: AnnotPrefer;
}

export interface ContainerNode extends NodeBase {
  type: "group" | "grid" | "stack" | "reaction" | "network";
}

/** Canonical nested figure document (root node). */
export type PictNode =
  | MolNode
  | ArrowNode
  | TextNode
  | ImageNode
  | TableNode
  | RefNode
  | AnnotationNode
  | ContainerNode;

/** @deprecated Flat molecule payload — use MolNode in nested trees. */
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
  /**
   * Molecule-scoped aliases: name → atom index or atom list.
   * Must not collide with `rings` keys.
   */
  ids?: Record<string, number | number[]>;
  /** Named rings: name → ordered atom indices. */
  rings?: Record<string, number[]>;
  /**
   * Labels for `*` atoms. List = star order (`null` = bare *);
   * dict = star ordinal → label.
   */
  rgroups?: (string | null)[] | Record<string, string | null>;
  /**
   * Labels for `*` atoms in layout encounter order (chem markup OK).
   * Wins over `rgroups` / CX aliases when set.
   */
  star_labels?: (string | null)[];
  /** Uniform diagram scale (`1` = house size). */
  scale?: number;
  /** Ink weight relative to house (`1`). Min `2/3` (Regular stem). */
  weight?: number;
  /** Ring-centered attachments (uncertain site); label required. */
  ring_attachments?: RingAttachmentSpec[];
  /** R-group table (`rows` matrix or `{groups, rows}`). */
  rtable?: string[][] | RTableSpec;
  marks?: MarkSpec[];
  /** Callouts, region shapes (box/oval/spline), and labels. */
  annotations?: AnnotationSpec[];
  shade?: ShadeSpec;
  /** Ink color for backbone bonds and atom labels (CSS). Default `#111`. */
  color?: string;
}

/** Ring-centered Markush attachment (`<r>`-style). */
export interface RingAttachmentSpec {
  /** Ring name from `rings`, or ordered atom indices. */
  ring: number[] | string;
  label: string;
  prefer?: AnnotPrefer;
}

/** R-group enumeration table. */
export interface RTableSpec {
  groups?: string[];
  rows?: string[][];
}

export interface MarkSpec {
  kind?: "atoms" | "bonds" | "substructure";
  atoms?: number[];
  bonds?: [number, number][];
  color?: string;
  label?: string;
}

/** How an annotation is drawn. */
export type AnnotKind = "callout" | "box" | "oval" | "spline";

/** Preferred callout side; collision grid may pick another free slot. */
export type AnnotPrefer = "auto" | "left" | "right" | "top" | "bottom";

/**
 * Molecule annotation with collision-aware label placement.
 *
 * Targets (use one): `atoms`, `bonds`, or `ring`.
 * Kinds: `callout` (arrow/text) or region `box` / `oval` / `spline`.
 */
export interface AnnotationSpec {
  kind?: AnnotKind;
  atoms?: number[];
  bonds?: [number, number][];
  /** Ordered atom indices of a ring (callout at centroid). */
  ring?: number[];
  /** Caption (supports light TeX/markdown markup). */
  label?: string;
  color?: string;
  /** Draw indicator arrow for callout annotations. */
  arrow?: boolean;
  prefer?: AnnotPrefer;
}

export interface ShadeSpec {
  atoms?: number[];
  bonds?: number[];
  colormap?: string;
  /** Shade window low (default 0). Not auto-scaled from data. */
  vmin?: number;
  /** Shade window high (default 1). Not auto-scaled from data. */
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

/** Legacy flat document (`{ molecules, diagram }`). */
export interface LegacyPictSpec {
  molecules: MoleculeSpec[];
  diagram?: DiagramSpec;
  width?: number;
  height?: number;
  /**
   * Global white knockout under all ink. On = everything; off = nothing.
   * Halos sit on the bottom layer and never obscure drawn content.
   */
  halo?: boolean;
}

/** Nested node tree or legacy flat document. */
export type PictSpec = PictNode | LegacyPictSpec;

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
  format?: OutputFormat;
  backend?: string;
}
