/**
 * MVP public surface for xenosite: plain JSON shapes + ``xpict`` namespace.
 * RDKit stays hidden (auto script in browser / npm on Node).
 *
 * ```ts
 * import { xpict } from "@swamidasslab/xpict";
 *
 * await xpict.init();
 * const mol = xpict.mol("CCCC");
 * const rendered = await xpict.render(mol);
 * // tweak rendered.scene if needed, then:
 * const svg = xpict.toSvg(rendered.scene);
 *
 * const aligned = await xpict.render(xpict.mol("CCCO"), { align_to: mol });
 * // or: { align_to: rendered }
 * ```
 */

import {
  layoutWithRdkit,
  materializeTemplateMolblock,
  type MoleculeIn,
} from "./layout/rdkit-layout.js";
import {
  ensureRdkit,
  isRdkitReady,
  type RdkitLoadOptions,
} from "./rdkit-loader.js";
import {
  depictMolecule,
  initNative,
  isNativeReady,
  type InitInput,
} from "./native.js";
import { atomsInSvgFrame, type SvgAtom, type SvgBond } from "./frame.js";
import {
  sceneToImgDataUri,
  sceneToSvg,
  type Scene,
} from "./draw/scene-svg.js";

export type { SvgAtom, SvgBond } from "./frame.js";
export type { Scene, ScenePrimitive, SceneLayer, SceneViewport } from "./draw/scene-svg.js";

export type InitOptions = RdkitLoadOptions & {
  /** Pass-through for xpict-core.wasm (Node usually needs bytes). */
  wasm?: InitInput | { module_or_path: InitInput | Promise<InitInput> };
};

/** Input molecule — SMILES/molfile plus optional cached alignment frame. */
export type Mol = {
  source: string;
  /**
   * Coord-bearing molblock for the home frame.
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
  mark_atoms?: number[];
  mark_bonds?: Array<[number, number]>;
  align_to?: AlignTarget;
};

let initPromise: Promise<void> | null = null;

async function init(opts: InitOptions = {}): Promise<void> {
  if (isNativeReady() && isRdkitReady()) return;
  if (!initPromise) {
    initPromise = (async () => {
      await Promise.all([
        ensureRdkit({
          scriptUrl: opts.scriptUrl,
          locateFile: opts.locateFile,
        }),
        initNative(opts.wasm),
      ]);
    })().catch((err) => {
      initPromise = null;
      throw err;
    });
  }
  await initPromise;
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
  await init();
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

function applyOpts(molecule: MoleculeIn, opts: MolRenderOptions): MoleculeIn {
  const out: MoleculeIn = {
    ...molecule,
    atoms: [...molecule.atoms],
    bonds: [...molecule.bonds],
  };
  if (opts.id !== undefined) out.id = opts.id;
  if (opts.color !== undefined) out.color = opts.color;
  if (opts.atom_shade !== undefined) out.atom_shade = opts.atom_shade;
  if (opts.bond_shade !== undefined) out.bond_shade = opts.bond_shade;
  if (opts.mark_atoms !== undefined) out.mark_atoms = opts.mark_atoms;
  if (opts.mark_bonds !== undefined) out.mark_bonds = opts.mark_bonds;
  return out;
}

function toCoordList(
  atoms: Array<{
    index: number;
    element: string;
    x: number;
    y: number;
    label?: string;
    charge?: number;
  }>
): SvgAtom[] {
  return atoms.map((a) => ({
    index: a.index,
    element: a.element,
    x: a.x,
    y: a.y,
    ...(a.label ? { label: a.label } : {}),
    ...(a.charge ? { charge: a.charge } : {}),
  }));
}

async function render(
  input: Mol | string,
  opts: MolRenderOptions = {}
): Promise<Rendered> {
  await init();
  const m: Mol = typeof input === "string" ? mol(input) : input;

  let laid: MoleculeIn;
  let poseMolblock: string;
  if (opts.align_to) {
    const template = await ensureFrame(opts.align_to);
    const result = await layoutWithRdkit(m.source, {
      template,
      id: opts.id,
    });
    laid = result.molecule;
    poseMolblock = result.molblock;
  } else {
    const home = await ensureFrame(m);
    const result = await layoutWithRdkit(home, { id: opts.id, template: null });
    laid = result.molecule;
    poseMolblock = result.molblock;
    if (!m.frame_molblock) m.frame_molblock = poseMolblock;
  }

  const molecule = applyOpts(laid, opts);
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

/** Public lib namespace — MVP surface for xenosite. */
export const xpict = {
  init,
  mol,
  render,
  /** Scene JSON → SVG string (tweak ``rendered.scene`` first if needed). */
  toSvg: sceneToSvg,
  /** Scene JSON → ``data:image/svg+xml`` URI for ``<img src>``. */
  toImgDataUri: sceneToImgDataUri,
} as const;
