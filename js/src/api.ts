/**
 * Public surface: single-mol client + declarative document (same paint).
 *
 * Single molecule:
 * ```ts
 * const mol = xpict.mol("CCCC");
 * const rendered = await xpict.render(mol, { color: "#0b6e4f" });
 * const svg = xpict.toSvg(rendered.scene);
 * const aligned = await xpict.render(xpict.mol("CCCO"), { align_to: mol });
 * ```
 *
 * Declarative document (or a single mol — wrapped as a mol spec):
 * ```ts
 * const [r] = await xpict.depict({
 *   type: "mol",
 *   cxsmiles: "*c1ccccc1Cl |$R1;;;;;$|",
 * });
 * const [ethanol] = await xpict.depict("CCO");
 * const svg = xpict.toSvg(r.scene);
 * ```
 */

import { initNative, isNativeReady, renderJson } from "./native.js";
import type { SvgAtom, SvgBond } from "./frame.js";
import { sceneToSvg, type Scene } from "./draw/scene-svg.js";
import type { MoleculeIn } from "./generated/edge.js";
import type { AtomLabelSpec } from "./generated/AtomLabelSpec.js";
import type { AtomNumberSpec } from "./generated/AtomNumberSpec.js";
import type { CalloutField } from "./generated/CalloutField.js";
import type { CircleField } from "./generated/CircleField.js";
import type { MolText } from "./generated/MolText.js";
import type { RegionField } from "./generated/RegionField.js";
import type { ShadeSpec } from "./generated/ShadeSpec.js";

export type { SvgAtom, SvgBond } from "./frame.js";
export type {
  Scene,
  ScenePrimitive,
  SceneLayer,
  SceneViewport,
  Primitive,
  Layer,
  Viewport,
  LayerName,
  TextAnchor,
} from "./draw/scene-svg.js";

/** Input molecule — SMILES/molfile plus optional cached alignment frame. */
export type Mol = {
  source: string;
  /**
   * Pose molblock filled on first render (debug / hosts). Align does not
   * require it — Rust layouts from the source.
   */
  frame_molblock?: string;
  id?: string;
};

/**
 * Rendered depiction. ``scene`` is the editable JSON drawable; convert with
 * ``xpict.toSvg(scene)`` when you want a string.
 */
export type Rendered = {
  width: number;
  height: number;
  /** ViewBox / SCALE positions — match scene / SVG ink. */
  svg_coords: SvgAtom[];
  /** SCALE layout (mean bond ≈ 20), before viewBox pad. */
  coords: SvgAtom[];
  bonds: SvgBond[];
  /** Editable scene document (paths, circles, …). */
  scene: Scene;
  molecule: MoleculeIn;
  source: string;
  /** Pose molblock from CoordGen (debug / hosts). */
  frame_molblock: string;
  mol?: Mol;
};

export type AlignTarget = Mol | Rendered;

export type MolRenderOptions = {
  id?: string;
  color?: string;
  atom_shade?: number[];
  bond_shade?: number[];
  /** Shade window low (default 0). Not inferred from data. */
  shade_vmin?: number;
  /** Shade window high (default 1). Not inferred from data. */
  shade_vmax?: number;
  mark_atoms?: number[];
  mark_bonds?: Array<[number, number]>;
  /**
   * Labels for ``*`` atoms in layout encounter order.
   * ``null`` / ``"*"`` → bare star glyph; other strings (e.g. ``R1``) replace it.
   * When omitted, CXSMILES ``|$alias;…$|`` trailers are applied by **atom
   * index** (parity with ``xpict_core::cx_atom_labels``).
   */
  star_labels?: Array<string | null>;
  /**
   * Ink weight relative to house size (`1` = default). Min `2/3` (Regular
   * stem); typical thicken up to ~2.
   */
  weight?: number;
  /** Uniform diagram scale (`1` = house size). */
  scale?: number;
  /** Aromatic rings: `circle` (default) or `kekule`. */
  aromatic?: "circle" | "kekule";
  /** Dashed layout boxes for labels / mol / scene (default off). */
  debug_box?: boolean;
  /** Alignment correspondence overlays (template atom index tags; default off). */
  debug_align?: boolean;
  align_to?: AlignTarget;
  /** Pairs `[queryAtom, templateAtom]`. Requires `align_to`; skips MCS. */
  atom_map?: Array<[number, number]>;
  name?: MolText;
  atom_number?: AtomNumberSpec;
  atom_label?: AtomLabelSpec;
  region?: RegionField;
  circle?: CircleField;
  callout?: CalloutField;
  shade?: ShadeSpec;
};

/**
 * Mol / group document types — generated from ``xpict-core`` (``make types``).
 * Star / Markush text: ``star_labels`` (encounter order) or CX aliases on
 * ``cxsmiles``. Document ``rgroups`` is not public yet.
 */
export type {
  AlignTo,
  AlignToSpec,
  MolNode,
  ShadeSpec,
  ShadeStyle,
  CommonOpts,
  MolOpts,
  NodeType,
  ForTypesPatch,
  TypedOptsPatch,
  OptsPatch,
  Opts,
  EdgeArrow,
  EdgeNode,
  TextNode,
  LabelPos,
  LabelPlacement,
  LabelItem,
  LabelLanes,
  Label,
  MolIds,
  LayoutDirection,
  EdgeRouting,
  LayoutAlgorithm,
  LayoutOpts,
  Node,
} from "./generated/depict-abi.js";
export type { AtomLabelSpec } from "./generated/AtomLabelSpec.js";
export type { AtomNumberSpec } from "./generated/AtomNumberSpec.js";
export type { CalloutField } from "./generated/CalloutField.js";
export type { CircleField } from "./generated/CircleField.js";
export type { RegionField } from "./generated/RegionField.js";
export type { MolText } from "./generated/MolText.js";

import type {
  DepictSpec as GeneratedDepictSpec,
  MolNode,
} from "./generated/depict-abi.js";

/**
 * Group — ``children`` of mol | text (no edges).
 * ``align`` defaults to false on the wire (`#[serde(default)]`); optional here
 * so callers may omit it (ts-rs cannot mark non-Option defaults optional).
 */
export type GroupNode = Omit<
  Extract<GeneratedDepictSpec, { type: "group" }>,
  "align"
> & {
  align?: boolean;
};

/** Reaction scheme — mol | edge | text children; layout via LayoutOpts. */
export type ReactionSchemeNode = Extract<
  GeneratedDepictSpec,
  { type: "reaction_scheme" }
>;

/** Grid of row roots. */
export type GridNode = Extract<GeneratedDepictSpec, { type: "grid" }>;

/** Declarative document (nested subset of PictSpec). Root mol is {@link MolNode}. */
export type DepictSpec =
  | MolNode
  | GroupNode
  | ReactionSchemeNode
  | GridNode;

/** Alias of {@link MolNode}. */
export type MolSpec = MolNode;

let readyPromise: Promise<void> | null = null;

/** Internal: load wasm once (from first render). */
async function ensureReady(): Promise<void> {
  if (isNativeReady()) return;
  if (!readyPromise) {
    readyPromise = initNative().catch((err) => {
      readyPromise = null;
      throw err;
    });
  }
  await readyPromise;
}

function mol(smilesOrMolfile: string): Mol {
  const text = smilesOrMolfile.trim();
  if (!text) throw new Error("mol() requires a non-empty SMILES or molfile");
  return { source: text };
}

function isRendered(value: AlignTarget): value is Rendered {
  return (
    typeof value === "object" &&
    value !== null &&
    "scene" in value &&
    "source" in value
  );
}

function structureFields(source: string): Pick<MolNode, "smiles" | "cxsmiles" | "molfile"> {
  const text = source.trim();
  if (text.includes("V2000") || text.includes("V3000") || text.split("\n").length >= 4) {
    return { molfile: text };
  }
  if (text.includes("|")) return { cxsmiles: text };
  return { smiles: text };
}

function shadeFromOpts(opts: MolRenderOptions): ShadeSpec | undefined {
  if (opts.shade) return opts.shade;
  if (opts.atom_shade === undefined && opts.bond_shade === undefined) return undefined;
  return {
    atoms: opts.atom_shade,
    bonds: opts.bond_shade,
    vmin: opts.shade_vmin ?? 0,
    vmax: opts.shade_vmax ?? 1,
  };
}

function molNodeFrom(source: string, opts: MolRenderOptions, id?: string): MolNode {
  const node: MolNode = {
    type: "mol",
    ...structureFields(source),
  };
  if (id) node.id = id;
  else if (opts.id) node.id = opts.id;
  if (opts.color !== undefined) node.color = opts.color;
  if (opts.weight !== undefined) node.weight = opts.weight;
  if (opts.scale !== undefined) node.scale = opts.scale;
  if (opts.aromatic !== undefined) node.aromatic = opts.aromatic;
  if (opts.debug_box !== undefined) node.debug_box = opts.debug_box;
  if (opts.debug_align !== undefined) node.debug_align = opts.debug_align;
  if (opts.star_labels !== undefined) node.star_labels = opts.star_labels;
  if (opts.name !== undefined) node.name = opts.name;
  if (opts.atom_number !== undefined) node.atom_number = opts.atom_number;
  if (opts.atom_label !== undefined) node.atom_label = opts.atom_label;
  if (opts.region !== undefined) node.region = opts.region;
  if (opts.circle !== undefined) node.circle = opts.circle;
  if (opts.callout !== undefined) node.callout = opts.callout;
  const shade = shadeFromOpts(opts);
  if (shade) node.shade = shade;
  return node;
}

function alignRefId(target: AlignTarget): string {
  if (isRendered(target)) {
    return target.mol?.id || target.molecule.id || "m_0";
  }
  return target.id || "m_0";
}

function alignSource(target: AlignTarget): string {
  return isRendered(target) ? target.source : target.source;
}

function specFromMol(input: Mol | string, opts: MolRenderOptions): DepictSpec {
  const m: Mol = typeof input === "string" ? mol(input) : input;
  if (opts.atom_map && !opts.align_to) {
    throw new Error("atom_map requires align_to");
  }
  if (!opts.align_to) {
    return molNodeFrom(m.source, opts, opts.id || m.id);
  }
  const refId = alignRefId(opts.align_to);
  let qid = opts.id || m.id || "m_1";
  if (qid === refId) qid = "m_1";
  const query = molNodeFrom(m.source, opts, qid);
  if (opts.atom_map) {
    query.align_to = {
      ref: refId,
      atom_map: opts.atom_map.map(([q, r]) => [q, r] as [number, number]),
    };
  } else {
    query.align_to = refId;
  }
  return {
    type: "group",
    align: false,
    children: [molNodeFrom(alignSource(opts.align_to), {}, refId), query],
  };
}

type NativeRendered = {
  id: string;
  width: number;
  height: number;
  scene: Scene;
  molecule: MoleculeIn;
  source: string;
  frame_molblock: string;
  coords: SvgAtom[];
  svg_coords: SvgAtom[];
  bonds: SvgBond[];
};

function asRendered(row: NativeRendered, m?: Mol): Rendered {
  const out: Rendered = {
    width: row.width,
    height: row.height,
    scene: row.scene,
    molecule: row.molecule,
    source: row.source,
    frame_molblock: row.frame_molblock ?? "",
    coords: row.coords ?? [],
    svg_coords: row.svg_coords ?? [],
    bonds: row.bonds ?? [],
  };
  if (m) out.mol = m;
  return out;
}

function isDepictSpec(value: unknown): value is DepictSpec {
  if (!value || typeof value !== "object") return false;
  const t = (value as { type?: unknown }).type;
  return t === "mol" || t === "group" || t === "reaction_scheme" || t === "grid";
}

async function runSpec(spec: DepictSpec): Promise<NativeRendered[]> {
  await ensureReady();
  return JSON.parse(renderJson(JSON.stringify(spec))) as NativeRendered[];
}

async function render(
  input: Mol | string,
  opts: MolRenderOptions = {}
): Promise<Rendered> {
  const m: Mol = typeof input === "string" ? mol(input) : input;
  const rows = await runSpec(specFromMol(m, opts));
  if (!rows.length) throw new Error("render returned no molecules");
  const painted = asRendered(rows[rows.length - 1]!, m);
  if (!m.frame_molblock && !opts.align_to) {
    m.frame_molblock = painted.frame_molblock;
  }
  return painted;
}

/**
 * Single mol or declarative document → ``Rendered[]``.
 *
 * A ``Mol`` / SMILES / molfile is wrapped as ``{ type: "mol", … }`` and sent
 * to the same Rust ``render(spec)`` as a full group / scheme.
 */
async function depict(
  input: DepictSpec | Mol | string,
  opts: MolRenderOptions = {}
): Promise<Rendered[]> {
  const spec = isDepictSpec(input) ? input : specFromMol(input, opts);
  const rows = await runSpec(spec);
  return rows.map((row) => asRendered(row));
}

/** Public lib namespace. */
export const xpict = {
  mol,
  render,
  /** Scene JSON → SVG string (tweak ``rendered.scene`` first if needed). */
  toSvg: sceneToSvg,
  /** Single mol or declarative document → ``Rendered[]``. */
  depict,
} as const;
