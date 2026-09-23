/**
 * Public surface: single-mol client + declarative document (same paint).
 * RDKit stays hidden (auto script in browser / npm on Node).
 *
 * Single molecule:
 * ```ts
 * const mol = xpict.mol("CCCC");
 * const rendered = await xpict.render(mol, { color: "#0b6e4f" });
 * const svg = xpict.toSvg(rendered.scene);
 * const aligned = await xpict.render(xpict.mol("CCCO"), { align_to: mol });
 * ```
 *
 * Declarative document (expanding toward PictSpec):
 * ```ts
 * const [r] = await xpict.depict({
 *   type: "mol",
 *   cxsmiles: "*c1ccccc1Cl |$R1;;;;;$|",
 * });
 * const svg = xpict.toSvg(r.scene);
 * ```
 */

import {
  layoutWithRdkit,
  materializeTemplateMolblock,
  type MoleculeIn,
} from "./layout/rdkit-layout.js";
import { ensureRdkit, isRdkitReady } from "./rdkit-loader.js";
import {
  depictMolecule,
  initNative,
  isNativeReady,
  planEdgeJson,
  renderDocJson,
} from "./native.js";
import { atomsInSvgFrame, type SvgAtom, type SvgBond } from "./frame.js";
import { sceneToSvg, type Scene } from "./draw/scene-svg.js";
import { elementSymbol } from "./elements.js";
import { cxAtomLabels } from "./cxsmiles.js";
import {
  processEdgePlanWithFrames,
  type EdgePlan,
} from "./edge-plan.js";

export type { SvgAtom, SvgBond } from "./frame.js";
export type {
  Scene,
  ScenePrimitive,
  SceneLayer,
  SceneViewport,
} from "./draw/scene-svg.js";

/** Input molecule — SMILES/molfile plus optional cached alignment frame. */
export type Mol = {
  source: string;
  /**
   * Coord-bearing molblock for the alignment frame.
   * Filled on first render; carried for ``align_to``.
   */
  frame_molblock?: string;
};

/**
 * Rendered depiction. ``scene`` is the editable JSON drawable; convert with
 * ``xpict.toSvg(scene)`` when you want a string. Also an ``align_to`` target
 * via ``frame_molblock``.
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
  /** Pose molblock — for ``align_to: rendered``. */
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
   * index** (parity with Python ``cx_atom_labels`` / native backend).
   */
  star_labels?: Array<string | null>;
  /**
   * Ink weight relative to house size (`1` = default). Min `2/3` (Regular
   * stem); typical thicken up to ~2.
   */
  weight?: number;
  /** Uniform diagram scale (`1` = house size). */
  scale?: number;
  align_to?: AlignTarget;
  /** Pairs `[queryAtom, templateAtom]`. Requires `align_to`; skips MCS. */
  atom_map?: Array<[number, number]>;
};

/**
 * Mol node — strict subset of future PictSpec ``type: "mol"``.
 * Star / Markush text: ``star_labels`` (encounter order) or CX aliases on
 * ``cxsmiles``. Document ``rgroups`` is not public yet.
 */
export type MolNode = {
  type: "mol";
  smiles?: string;
  cxsmiles?: string;
  molfile?: string;
  id?: string;
  color?: string;
  /** Per-atom / per-bond colormap scores (document shade). */
  shade?: {
    atoms?: number[];
    bonds?: number[];
    colormap?: string;
    /** Shade window low (default 0). Not auto-scaled from data. */
    vmin?: number;
    /** Shade window high (default 1). Not auto-scaled from data. */
    vmax?: number;
  };
  /**
   * Labels for ``*`` atoms in layout encounter order (chem markup OK).
   * Wins over CXSMILES aliases when both are present.
   */
  star_labels?: Array<string | null>;
  /** Uniform diagram scale (`1` = house size). */
  scale?: number;
  /**
   * Ink weight relative to house (`1`). Min `2/3` (Regular stem).
   */
  weight?: number;
  /** Id of another mol in this group to use as align template. */
  align_to?: string;
  /** Pairs `[queryAtom, templateAtom]` vs the template; skips MCS when set. */
  atom_map?: Array<[number, number]>;
};

/** Group — ``children`` of mol nodes only (today). */
export type GroupNode = {
  type: "group";
  id?: string;
  /**
   * When true, later children align onto the first (or each child's
   * ``align_to``). Builds an EdgePlan coord_gen forest.
   */
  align?: boolean;
  children: MolNode[];
};

/** Declarative document (nested subset of PictSpec; still expanding). */
export type DepictSpec = MolNode | GroupNode;

/** Alias of {@link MolNode}. */
export type MolSpec = MolNode;

let readyPromise: Promise<void> | null = null;

/** Internal: load RDKit + wasm once (from first render). */
async function ensureReady(): Promise<void> {
  if (isNativeReady() && isRdkitReady()) return;
  if (!readyPromise) {
    readyPromise = (async () => {
      await Promise.all([ensureRdkit(), initNative()]);
    })().catch((err) => {
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
    "frame_molblock" in value &&
    typeof (value as Rendered).frame_molblock === "string"
  );
}

async function ensureFrame(target: AlignTarget): Promise<string> {
  await ensureReady();
  if (isRendered(target)) {
    if (!target.frame_molblock) {
      throw new Error("Rendered is missing frame_molblock");
    }
    return target.frame_molblock;
  }
  if (!target.frame_molblock) {
    target.frame_molblock = await materializeTemplateMolblock(target.source);
  }
  return target.frame_molblock;
}

function isStarAtom(a: {
  element?: string;
  z?: number;
}): boolean {
  if (a.element === "*") return true;
  if (a.z === 0) return true;
  return false;
}

/** Apply ``star_labels`` onto ``*`` atoms in layout encounter order. */
function applyStarLabels(
  molecule: MoleculeIn,
  labels: Array<string | null> | undefined
): MoleculeIn {
  if (!labels || labels.length === 0) return molecule;
  const stars = molecule.atoms
    .map((a, i) => ({ a, i }))
    .filter(({ a }) => isStarAtom(a));
  if (stars.length === 0) return molecule;
  const atoms = molecule.atoms.map((a) => ({ ...a }));
  for (let k = 0; k < labels.length && k < stars.length; k++) {
    const raw = labels[k];
    const label =
      raw === null || raw === undefined || String(raw).trim() === ""
        ? "*"
        : String(raw).trim();
    atoms[stars[k].i] = { ...atoms[stars[k].i], label };
  }
  return { ...molecule, atoms };
}

/**
 * Apply CXSMILES ``|$a;b;c;$|`` aliases by atom index (Python
 * ``cx_atom_labels`` / native backend parity). Only non-empty slots change
 * labels — so ``*C* |$;;R2;$|`` labels the second star, not the first.
 */
function applyCxLabelsByIndex(
  molecule: MoleculeIn,
  aliases: Array<string | null>
): MoleculeIn {
  if (!aliases.length) return molecule;
  const atoms = molecule.atoms.map((a) => {
    const raw = a.index < aliases.length ? aliases[a.index] : null;
    if (raw === null || raw === undefined) return a;
    const label = String(raw).trim();
    if (!label) return a;
    return { ...a, label };
  });
  return { ...molecule, atoms };
}

function applyOpts(
  molecule: MoleculeIn,
  opts: MolRenderOptions,
  source: string
): MoleculeIn {
  let out: MoleculeIn = {
    ...molecule,
    atoms: [...molecule.atoms],
    bonds: [...molecule.bonds],
  };
  if (opts.id !== undefined) out.id = opts.id;
  if (opts.color !== undefined) out.color = opts.color;
  if (opts.atom_shade !== undefined) out.atom_shade = opts.atom_shade;
  if (opts.bond_shade !== undefined) out.bond_shade = opts.bond_shade;
  if (opts.shade_vmin !== undefined) out.shade_vmin = opts.shade_vmin;
  if (opts.shade_vmax !== undefined) out.shade_vmax = opts.shade_vmax;
  if (opts.mark_atoms !== undefined) out.mark_atoms = opts.mark_atoms;
  if (opts.mark_bonds !== undefined) out.mark_bonds = opts.mark_bonds;
  if (opts.weight !== undefined) out.weight = opts.weight;
  if (opts.scale !== undefined) out.scale = opts.scale;
  // Explicit star_labels (encounter order) wins; otherwise CX by atom index.
  if (opts.star_labels !== undefined) {
    out = applyStarLabels(out, opts.star_labels);
  } else {
    out = applyCxLabelsByIndex(out, cxAtomLabels(source));
  }
  return out;
}

function toCoordList(
  atoms: Array<{
    index: number;
    element?: string;
    z?: number;
    x: number;
    y: number;
    label?: string;
    charge?: number;
  }>
): SvgAtom[] {
  return atoms.map((a) => {
    const element =
      a.element && a.element.length > 0
        ? a.element
        : a.z !== undefined
          ? elementSymbol(a.z)
          : "C";
    return {
      index: a.index,
      element,
      x: a.x,
      y: a.y,
      ...(a.label ? { label: a.label } : {}),
      ...(a.charge ? { charge: a.charge } : {}),
    };
  });
}

async function render(
  input: Mol | string,
  opts: MolRenderOptions = {}
): Promise<Rendered> {
  await ensureReady();
  const m: Mol = typeof input === "string" ? mol(input) : input;

  let laid: MoleculeIn;
  let poseMolblock: string;
  if (opts.atom_map && !opts.align_to) {
    throw new Error("atom_map requires align_to");
  }
  if (opts.align_to) {
    const template = await ensureFrame(opts.align_to);
    const result = await layoutWithRdkit(m.source, {
      template,
      id: opts.id,
      atomMap: opts.atom_map ?? null,
    });
    laid = result.molecule;
    poseMolblock = result.molblock;
  } else {
    // Layout from the SMILES/CX source (not a molblock round-trip) so dummy
    // ``*`` atoms keep stable indices for CX alias slots.
    const result = await layoutWithRdkit(m.source, {
      id: opts.id,
      template: null,
    });
    laid = result.molecule;
    poseMolblock = result.molblock;
    if (!m.frame_molblock) m.frame_molblock = poseMolblock;
  }

  const molecule = applyOpts(laid, opts, m.source);
  const sceneJson = depictMolecule(JSON.stringify(molecule));
  const scene = JSON.parse(sceneJson) as Scene;
  const framed = atomsInSvgFrame(molecule);

  return {
    width: scene.width,
    height: scene.height,
    svg_coords: framed.atoms,
    coords: toCoordList(molecule.atoms),
    bonds: framed.bonds,
    scene,
    molecule,
    source: m.source,
    frame_molblock: poseMolblock,
    mol: m,
  };
}

function structureFromMolNode(entry: MolNode): string {
  const raw =
    entry.molfile?.trim() ||
    entry.cxsmiles?.trim() ||
    entry.smiles?.trim() ||
    "";
  if (!raw) {
    throw new Error('mol node needs smiles, cxsmiles, or molfile');
  }
  return raw;
}

type DocPaint = {
  id: string;
  molecule: MoleculeIn;
  scene: Scene;
};

/**
 * Declarative document → ``Rendered[]``.
 * Core ``planEdge`` → host ``processEdgePlan`` → core ``renderDoc``
 * (CX / star / shade chrome applied in core).
 */
async function depict(spec: DepictSpec): Promise<Rendered[]> {
  await ensureReady();
  const plan = JSON.parse(planEdgeJson(JSON.stringify(spec))) as EdgePlan | null;
  if (plan == null) return [];
  const { result: edge, frames } = await processEdgePlanWithFrames(plan);
  const painted = JSON.parse(
    renderDocJson(JSON.stringify(spec), JSON.stringify(edge))
  ) as DocPaint[];

  const byId = new Map<string, MolNode>();
  const nodes =
    spec.type === "mol"
      ? [spec]
      : spec.type === "group"
        ? (spec.children ?? [])
        : [];
  for (const [i, n] of nodes.entries()) {
    byId.set(n.id?.trim() || `m_${i}`, n);
  }

  return painted.map((row) => {
    const node = byId.get(row.id);
    const source = node ? structureFromMolNode(node) : row.id;
    const framed = atomsInSvgFrame(row.molecule);
    return {
      width: row.scene.width,
      height: row.scene.height,
      scene: row.scene,
      molecule: row.molecule,
      source,
      frame_molblock: frames.get(row.id) ?? "",
      coords: toCoordList(row.molecule.atoms),
      svg_coords: framed,
      bonds: (row.molecule.bonds ?? []).map((b) => ({
        index: b.index,
        begin: b.begin,
        end: b.end,
        order: b.order,
        ...(b.stereo ? { stereo: b.stereo } : {}),
      })),
    };
  });
}

/** Public lib namespace. */
export const xpict = {
  mol,
  render,
  /** Scene JSON → SVG string (tweak ``rendered.scene`` first if needed). */
  toSvg: sceneToSvg,
  /** Declarative document → ``Rendered[]`` (expanding toward PictSpec). */
  depict,
} as const;
