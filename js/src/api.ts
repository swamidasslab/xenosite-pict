/**
 * Tiny high-level browser/Node API for xenosite.
 *
 * Hides RDKit + wasm: one {@link init}, present a {@link presentTemplate
 * template mol}, then {@link draw} / {@link layout} / {@link depict}.
 * Alignment is RDKit `generate_aligned_coords` under the hood.
 *
 * Works in the browser (auto ``<script>`` inject) and on the server (Node
 * loads ``@rdkit/rdkit`` from npm — no DOM required).
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
import { sceneToImgDataUri, sceneToSvg, type Scene } from "./draw/scene-svg.js";

export type { MoleculeIn } from "./layout/rdkit-layout.js";
export type { SvgAtom, SvgBond } from "./frame.js";
export type { Scene } from "./draw/scene-svg.js";
export { SCALE } from "./layout/rdkit-layout.js";
export { sceneToSvg, sceneToImgDataUri } from "./draw/scene-svg.js";

export type InitOptions = RdkitLoadOptions & {
  /** Pass-through for xpict-core.wasm (Node usually needs bytes). */
  wasm?: InitInput | { module_or_path: InitInput | Promise<InitInput> };
};

export type DepictOptions = {
  /** Override the presented template for this call only. `null` skips align. */
  template?: string | null;
  id?: string;
  color?: string;
  atom_shade?: number[];
  bond_shade?: number[];
  mark_atoms?: number[];
  mark_bonds?: Array<[number, number]>;
};

/** Result of {@link draw}: SVG plus atom coords in the same viewBox space. */
export type DrawResult = {
  svg: string;
  /** data-URI for ``<img src>`` (xenosite pattern). */
  imgDataUri: string;
  width: number;
  height: number;
  /** Atom centers in SVG viewBox / SCALE units — match ink positions. */
  atoms: SvgAtom[];
  bonds: SvgBond[];
  scene: Scene;
  /** Pre-pad MoleculeIn (aligned, SCALE bond length). */
  molecule: MoleculeIn;
};

/** Author-facing template source (SMILES / molfile). */
let templateSource: string | null = null;
/** Coord-bearing molblock — stable frame for every align. */
let templateMolblock: string | null = null;
let initPromise: Promise<void> | null = null;

/** Load RDKit (script or npm) + xpict wasm. Idempotent. Browser + Node. */
export async function init(opts: InitOptions = {}): Promise<void> {
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

/**
 * Present the template mol everything else aligns to (SMILES or molfile).
 * Coords are materialized once so later molecules share one frame.
 */
export function presentTemplate(source: string): void {
  const text = source.trim();
  if (!text) throw new Error("presentTemplate requires a non-empty SMILES or molfile");
  templateSource = text;
  templateMolblock = null;
}

/** Clear the presented template (later layouts depict independently). */
export function clearTemplate(): void {
  templateSource = null;
  templateMolblock = null;
}

/** Currently presented template source, or `null`. */
export function currentTemplate(): string | null {
  return templateSource;
}

async function presentedTemplateMolblock(): Promise<string | null> {
  if (!templateSource) return null;
  if (!templateMolblock) {
    await init();
    templateMolblock = await materializeTemplateMolblock(templateSource);
  }
  return templateMolblock;
}

async function resolveTemplate(
  override?: string | null
): Promise<string | null | undefined> {
  if (override === undefined) return presentedTemplateMolblock();
  if (override === null) return null;
  return materializeTemplateMolblock(override);
}

/** RDKit 2D layout; aligns onto the presented (or passed) template when set. */
export async function layout(
  source: string,
  opts: { template?: string | null; id?: string } = {}
): Promise<MoleculeIn> {
  await init();
  const template = await resolveTemplate(opts.template);
  return layoutWithRdkit(source, { template, id: opts.id });
}

/** Layout the presented template itself (or `source` if given). */
export async function layoutPresentedTemplate(
  source?: string,
  opts: { id?: string } = {}
): Promise<MoleculeIn> {
  await init();
  if (source) {
    presentTemplate(source);
  }
  const mb = await presentedTemplateMolblock();
  if (!mb) throw new Error("no template presented — call presentTemplate() first");
  return layoutWithRdkit(mb, { id: opts.id, template: null });
}

function isMoleculeIn(value: unknown): value is MoleculeIn {
  return (
    !!value &&
    typeof value === "object" &&
    Array.isArray((value as MoleculeIn).atoms) &&
    Array.isArray((value as MoleculeIn).bonds)
  );
}

function applyDepictOpts(mol: MoleculeIn, opts: DepictOptions): MoleculeIn {
  const out: MoleculeIn = { ...mol, atoms: [...mol.atoms], bonds: [...mol.bonds] };
  if (opts.id !== undefined) out.id = opts.id;
  if (opts.color !== undefined) out.color = opts.color;
  if (opts.atom_shade !== undefined) out.atom_shade = opts.atom_shade;
  if (opts.bond_shade !== undefined) out.bond_shade = opts.bond_shade;
  if (opts.mark_atoms !== undefined) out.mark_atoms = opts.mark_atoms;
  if (opts.mark_bonds !== undefined) out.mark_bonds = opts.mark_bonds;
  return out;
}

async function resolveMolecule(
  source: string | MoleculeIn,
  opts: DepictOptions
): Promise<MoleculeIn> {
  if (isMoleculeIn(source)) {
    return applyDepictOpts(source, opts);
  }
  const laid = await layout(source, { template: opts.template, id: opts.id });
  return applyDepictOpts(laid, opts);
}

/**
 * Layout (optional template align) + `depictMolecule` → Scene JSON.
 * Prefer {@link draw} when you need SVG + matching atom coords.
 */
export async function depict(
  source: string | MoleculeIn,
  opts: DepictOptions = {}
): Promise<string> {
  await init();
  const mol = await resolveMolecule(source, opts);
  return depictMolecule(JSON.stringify(mol));
}

/**
 * High-level draw: SVG + atom/bond coords in the **same** SCALE / viewBox
 * space as the SVG ink (pad-translated to match ``depict_molecule``).
 */
export async function draw(
  source: string | MoleculeIn,
  opts: DepictOptions = {}
): Promise<DrawResult> {
  await init();
  const molecule = await resolveMolecule(source, opts);
  const sceneJson = depictMolecule(JSON.stringify(molecule));
  const scene = JSON.parse(sceneJson) as Scene;
  const framed = atomsInSvgFrame(molecule);
  const svg = sceneToSvg(scene);
  return {
    svg,
    imgDataUri: sceneToImgDataUri(scene),
    width: scene.width,
    height: scene.height,
    atoms: framed.atoms,
    bonds: framed.bonds,
    scene,
    molecule,
  };
}
