/**
 * RDKit layout + template align → {@link MoleculeIn} (SVG / SCALE space).
 *
 * Alignment uses MinimalLib `generate_aligned_coords` with an FMCS
 * `referenceSmarts` (`BondCompare: Any` so aromatic ↔ kekulé / quinone),
 * then a saturation filter so aliphatic rings do not match quinones.
 * Protocol mirrors Rust ``xpict::align_opts`` — do not reimplement Kabsch here.
 */

import { ensureRdkit, type RdkitMol, type RdkitModule } from "../rdkit-loader.js";
import { elementSymbol } from "../elements.js";

/** Bond length target in drawing units (matches Rust `SCALE` / Python). */
export const SCALE = 20;

/**
 * Keep in sync with ``xpict::align_opts::MCS_DETAILS_JSON``.
 * Element atoms; any-bond so aromatic ↔ kekulé / quinone. Callers must still
 * run {@link mcsSaturationCompatible} so aliphatic rings do not match quinones.
 */
export const MCS_DETAILS_JSON = JSON.stringify({
  AtomCompare: "Elements",
  BondCompare: "Any",
  Timeout: 2,
});

const MIN_MCS_ATOMS = 3;

type BondJson = { atoms: [number, number]; bo?: number };

function bondKey(a: number, b: number): string {
  return a < b ? `${a}-${b}` : `${b}-${a}`;
}

/** MinimalLib may return `number[][]` or `{atoms:number[]}[]`. */
function normalizeSubstructMatches(raw: string): number[][] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  const out: number[][] = [];
  for (const m of parsed) {
    if (Array.isArray(m) && m.every((x) => typeof x === "number")) {
      out.push(m as number[]);
    } else if (
      m &&
      typeof m === "object" &&
      Array.isArray((m as { atoms?: unknown }).atoms)
    ) {
      out.push((m as { atoms: number[] }).atoms);
    }
  }
  return out;
}

function molBondOrders(mol: RdkitMol): Map<string, number> {
  const json = JSON.parse(mol.get_json()) as RdkitMolJson;
  const bonds = (json.molecules[0]?.bonds ?? []) as BondJson[];
  const out = new Map<string, number>();
  for (const b of bonds) {
    out.set(bondKey(b.atoms[0], b.atoms[1]), b.bo ?? 1);
  }
  return out;
}

function matchHasUnsaturated(
  bondOrders: Map<string, number>,
  match: number[],
  patternBonds: BondJson[]
): boolean {
  for (const pb of patternBonds) {
    const a = match[pb.atoms[0]];
    const b = match[pb.atoms[1]];
    if (a === undefined || b === undefined) continue;
    const bo = bondOrders.get(bondKey(a, b)) ?? 1;
    if (bo >= 1.5) return true;
  }
  return false;
}

/**
 * Reject saturated-aliphatic ↔ unsaturated MCS (cyclohexane ↔ quinone).
 * Keep in sync with Python ``_mcs_saturation_compatible``.
 */
export function mcsSaturationCompatible(
  rdkit: RdkitModule,
  a: RdkitMol,
  b: RdkitMol,
  smarts: string
): boolean {
  const pattern = rdkit.get_qmol(smarts);
  if (!pattern || !pattern.is_valid()) {
    pattern?.delete();
    return false;
  }
  try {
    const patJson = JSON.parse(pattern.get_json()) as RdkitMolJson;
    const patternBonds = (patJson.molecules[0]?.bonds ?? []) as BondJson[];
    if (!patternBonds.length) return true;

    let matchesA: number[][] = [];
    let matchesB: number[][] = [];
    try {
      matchesA = normalizeSubstructMatches(a.get_substruct_matches(pattern));
      matchesB = normalizeSubstructMatches(b.get_substruct_matches(pattern));
    } catch {
      return false;
    }
    if (!matchesA.length || !matchesB.length) return false;

    const ordersA = molBondOrders(a);
    const ordersB = molBondOrders(b);
    for (const ma of matchesA) {
      const ua = matchHasUnsaturated(ordersA, ma, patternBonds);
      for (const mb of matchesB) {
        const ub = matchHasUnsaturated(ordersB, mb, patternBonds);
        if (ua === ub) return true;
      }
    }
    return false;
  } finally {
    pattern.delete();
  }
}

/** Keep in sync with ``xpict::align_opts::minimallib_align_details``. */
export function minimallibAlignDetails(referenceSmarts: string): string {
  return JSON.stringify({
    useCoordGen: false,
    referenceSmarts,
    allowRGroups: true,
    acceptFailure: false,
  });
}

/** Keep in sync with ``xpict::align_opts::align_succeeded``. */
export function alignSucceeded(result: string): boolean {
  const t = result.trim();
  return t.length > 0 && t !== "{}";
}

export type MoleculeIn = {
  id?: string;
  atoms: Array<{
    index: number;
    /** Optional — omit when ``z`` is set (Rust resolves the symbol). */
    element?: string;
    /** Atomic number (`0` = ``*``). Preferred from RDKit JSON. */
    z?: number;
    x: number;
    y: number;
    label?: string;
    charge?: number;
  }>;
  bonds: Array<{
    index: number;
    begin: number;
    end: number;
    order: number;
    stereo?: string;
  }>;
  color?: string;
  atom_shade?: number[];
  bond_shade?: number[];
  /** Shade window low (default 0). Not inferred from data. */
  shade_vmin?: number;
  /** Shade window high (default 1). Not inferred from data. */
  shade_vmax?: number;
  mark_atoms?: number[];
  mark_bonds?: Array<[number, number]>;
  /** Ink weight relative to house (`1`). Min `2/3`. Omitted → house. */
  weight?: number;
  /** Uniform diagram scale (`1` = house). Omitted → `1`. */
  scale?: number;
};

type RdkitAtomJson = {
  z?: number;
  chg?: number;
  impHs?: number;
  isotope?: number;
};
type RdkitBondJson = {
  atoms: [number, number];
  bo?: number;
};
type RdkitMolJson = {
  molecules: Array<{
    atoms: RdkitAtomJson[];
    bonds: RdkitBondJson[];
  }>;
};

function elementFromZ(z: number | undefined): string {
  if (z === undefined) return "C";
  return elementSymbol(z);
}

function atomLabel(element: string, impHs: number, charge: number): string | undefined {
  if (element === "C" && !charge) return undefined;
  if (element === "*") return "*";
  let text = element;
  if (impHs === 1) text = `${element}H`;
  else if (impHs > 1) text = `${element}H${impHs}`;
  if (charge) {
    const sign = charge > 0 ? "+" : "−";
    const mag = Math.abs(charge);
    text = mag === 1 ? `${text}${sign}` : `${text}${mag}${sign}`;
  }
  return text;
}

function meanBondLength(
  coords: Array<[number, number]>,
  bonds: RdkitBondJson[]
): number {
  const lengths: number[] = [];
  for (const b of bonds) {
    const [i, j] = b.atoms;
    const a = coords[i];
    const c = coords[j];
    if (!a || !c) continue;
    lengths.push(Math.hypot(a[0] - c[0], a[1] - c[1]));
  }
  if (!lengths.length) return 1.5;
  const mean = lengths.reduce((s, x) => s + x, 0) / lengths.length;
  return mean < 1e-6 ? 1.5 : mean;
}

function parseCoords(mol: RdkitMol): Array<[number, number]> {
  const raw = mol.get_coords() as number[][];
  if (!Array.isArray(raw) || raw.length === 0) {
    throw new Error("RDKit molecule has no 2D coordinates");
  }
  return raw.map((p) => [Number(p[0]), Number(p[1])] as [number, number]);
}

function toMoleculeIn(
  mol: RdkitMol,
  opts: { id?: string; scale?: number; flipMaxY?: number } = {}
): MoleculeIn {
  const json = JSON.parse(mol.get_json()) as RdkitMolJson;
  const entry = json.molecules[0];
  if (!entry) throw new Error("RDKit JSON missing molecule");
  const coords = parseCoords(mol);
  const scale = opts.scale ?? SCALE / meanBondLength(coords, entry.bonds);

  let maxY = opts.flipMaxY;
  if (maxY === undefined) {
    maxY = -Infinity;
    for (const [, y] of coords) maxY = Math.max(maxY, y);
  }

  const atoms: MoleculeIn["atoms"] = entry.atoms.map((a, index) => {
    const z = a.z ?? 6;
    const element = elementFromZ(a.z);
    const charge = a.chg ?? 0;
    const impHs = a.impHs ?? 0;
    const [x0, y0] = coords[index] ?? [0, 0];
    // RDKit is Y-up; SVG / depict is Y-down. Shared flipMaxY keeps a
    // template frame stable across aligned molecules.
    const x = x0 * scale;
    const y = (maxY! - y0) * scale;
    const label = atomLabel(element, impHs, charge);
    return {
      index,
      z,
      x,
      y,
      ...(charge ? { charge } : {}),
      ...(label ? { label } : {}),
    };
  });

  const bonds: MoleculeIn["bonds"] = entry.bonds.map((b, index) => ({
    index,
    begin: b.atoms[0],
    end: b.atoms[1],
    order: b.bo ?? 1,
  }));

  return {
    ...(opts.id ? { id: opts.id } : {}),
    atoms,
    bonds,
  };
}

function getMol(rdkit: RdkitModule, source: string): RdkitMol {
  const mol = rdkit.get_mol(source);
  if (!mol || !mol.is_valid()) {
    mol?.delete();
    throw new Error(`RDKit could not parse molecule: ${source.slice(0, 80)}`);
  }
  try {
    mol.convert_to_kekule_form();
  } catch {
    // keep aromatic if kekulize fails
  }
  return mol;
}

function ensureCoords(mol: RdkitMol): void {
  if (!mol.has_coords()) {
    if (!mol.set_new_coords()) {
      throw new Error("RDKit failed to generate 2D coordinates");
    }
    mol.normalize_depiction();
  }
  // Molfile / cached templates already carry coords — leave the frame alone.
}

/** FMCS SMARTS for MinimalLib (same JSON as Rust ``MCS_DETAILS_JSON``). */
function mcsReferenceSmarts(
  rdkit: RdkitModule,
  a: RdkitMol,
  b: RdkitMol
): string | null {
  const list = new rdkit.MolList();
  list.append(a);
  list.append(b);
  const raw = rdkit.get_mcs_as_json(list, MCS_DETAILS_JSON);
  let parsed: { numAtoms?: number; canceled?: boolean; smarts?: string };
  try {
    parsed = JSON.parse(raw) as typeof parsed;
  } catch {
    return null;
  }
  if (
    parsed.canceled ||
    !parsed.smarts ||
    (parsed.numAtoms ?? 0) < MIN_MCS_ATOMS
  ) {
    return null;
  }
  if (!mcsSaturationCompatible(rdkit, a, b, parsed.smarts)) {
    return null;
  }
  return parsed.smarts;
}

export type LayoutResult = {
  molecule: MoleculeIn;
  /** Coord-bearing molblock of the laid-out mol — pack into Rendered for align_to. */
  molblock: string;
};

/**
 * Layout `source` (SMILES / molfile). When `template` is set, RDKit MCS-aligns
 * onto that pose via `generate_aligned_coords` + `referenceSmarts`.
 */
export async function layoutWithRdkit(
  source: string,
  opts: { template?: string | null; id?: string } = {}
): Promise<LayoutResult> {
  const rdkit = await ensureRdkit();
  const mol = getMol(rdkit, source);
  let templateMol: RdkitMol | null = null;
  try {
    if (opts.template) {
      templateMol = getMol(rdkit, opts.template);
      ensureCoords(templateMol);

      const smarts = mcsReferenceSmarts(rdkit, mol, templateMol);
      let aligned = "";
      if (smarts) {
        aligned = mol.generate_aligned_coords(
          templateMol,
          minimallibAlignDetails(smarts)
        );
      }
      if (!alignSucceeded(aligned)) {
        // No usable MCS / match — free layout (do not pretend we aligned).
        ensureCoords(mol);
        return {
          molecule: toMoleculeIn(mol, { id: opts.id }),
          molblock: sanitizeDummyMolblock(mol.get_molblock()),
        };
      }

      const tmplJson = JSON.parse(templateMol.get_json()) as RdkitMolJson;
      const tmplBonds = tmplJson.molecules[0]?.bonds ?? [];
      const tmplCoords = parseCoords(templateMol);
      const tmplScale = SCALE / meanBondLength(tmplCoords, tmplBonds);
      let flipMaxY = -Infinity;
      for (const [, y] of tmplCoords) flipMaxY = Math.max(flipMaxY, y);
      return {
        molecule: toMoleculeIn(mol, { id: opts.id, scale: tmplScale, flipMaxY }),
        molblock: sanitizeDummyMolblock(mol.get_molblock()),
      };
    }
    ensureCoords(mol);
    return {
      molecule: toMoleculeIn(mol, { id: opts.id }),
      molblock: sanitizeDummyMolblock(mol.get_molblock()),
    };
  } finally {
    mol.delete();
    templateMol?.delete();
  }
}

/**
 * MinimalLib writes dummy ``*`` as molfile ``R`` plus an ``M  ALS`` query that
 * re-parses the first dummy as hydrogen. Rewrite dummies as ``*`` and drop ALS
 * so multi-star frames round-trip with stable indices.
 */
export function sanitizeDummyMolblock(molblock: string): string {
  return molblock
    .split("\n")
    .filter((line) => !line.startsWith("M  ALS"))
    .map((line) => line.replace(/ R /g, " * "))
    .join("\n");
}

/**
 * Build a coord-bearing molblock for use as a stable alignment template.
 * Call once when presenting; reuse the molblock for every later align.
 */
export async function materializeTemplateMolblock(source: string): Promise<string> {
  const rdkit = await ensureRdkit();
  const mol = getMol(rdkit, source);
  try {
    if (!mol.has_coords()) {
      if (!mol.set_new_coords()) {
        throw new Error("RDKit failed to generate template coordinates");
      }
      mol.normalize_depiction();
    }
    return sanitizeDummyMolblock(mol.get_molblock());
  } finally {
    mol.delete();
  }
}
