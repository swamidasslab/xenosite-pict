/**
 * RDKit layout + template align → {@link MoleculeIn} (SVG / SCALE space).
 *
 * Alignment uses MinimalLib `generate_aligned_coords` (same idea as Python
 * `align_rdkit`). Callers go through `api.ts`; this module stays internal.
 */

import { ensureRdkit, type RdkitMol, type RdkitModule } from "../rdkit-loader.js";
import { elementSymbol } from "../elements.js";

/** Bond length target in drawing units (matches Rust `SCALE` / Python). */
export const SCALE = 20;

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
  mark_atoms?: number[];
  mark_bonds?: Array<[number, number]>;
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

const ALIGN_OPTS = JSON.stringify({
  useCoordGen: false,
  allowOptionalAttachments: true,
  acceptFailure: true,
});

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

export type LayoutResult = {
  molecule: MoleculeIn;
  /** Coord-bearing molblock of the laid-out mol — pack into Rendered for align_to. */
  molblock: string;
};

/**
 * Layout `source` (SMILES / molfile). When `template` is set, RDKit aligns
 * onto that frame via `generate_aligned_coords`.
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
      const aligned = mol.generate_aligned_coords(templateMol, ALIGN_OPTS);
      if (!aligned) {
        ensureCoords(mol);
        return {
          molecule: toMoleculeIn(mol, { id: opts.id }),
          molblock: mol.get_molblock(),
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
        molblock: mol.get_molblock(),
      };
    }
    ensureCoords(mol);
    return {
      molecule: toMoleculeIn(mol, { id: opts.id }),
      molblock: mol.get_molblock(),
    };
  } finally {
    mol.delete();
    templateMol?.delete();
  }
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
    return mol.get_molblock();
  } finally {
    mol.delete();
  }
}
