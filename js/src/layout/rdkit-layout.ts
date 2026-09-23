/**
 * RDKit layout + template align → {@link MoleculeIn} (SVG / SCALE space).
 *
 * MCS uses element + hybridization (isotope-encoded ``Z×10+hyb`` on copies;
 * ``AtomCompare: Isotopes``) and ``BondCompare: Any``. Align calls MinimalLib
 * ``generate_aligned_coords`` with the MCS isotope ``referenceSmarts`` on those
 * same tagged copies (MinimalLib has no atom-map overload — that is Python /
 * native Depictor). Protocol mirrors Rust ``xpict::align_opts``.
 */

import { ensureRdkit, type RdkitMol, type RdkitModule } from "../rdkit-loader.js";
import { elementSymbol } from "../elements.js";

/** Bond length target in drawing units (matches Rust `SCALE` / Python). */
export const SCALE = 20;

/**
 * Keep in sync with ``xpict::align_opts::MCS_DETAILS_JSON``.
 * Run on mols tagged with {@link tagHybridizationIsotopes}.
 */
export const MCS_DETAILS_JSON = JSON.stringify({
  AtomCompare: "Isotopes",
  BondCompare: "Any",
  Timeout: 2,
});

const MIN_MCS_ATOMS = 3;

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

/**
 * Infer SP2 (2) vs SP3 (3) from MinimalLib bond orders (often kekulized).
 * Heteroatoms on an unsaturated neighbor count as SP2 (phenol / aniline).
 */
export function inferHybridizationCodes(molJson: RdkitMolJson): number[] {
  const mol0 = molJson.molecules[0];
  const atoms = mol0?.atoms ?? [];
  const bonds = mol0?.bonds ?? [];
  const n = atoms.length;
  const maxBo = Array<number>(n).fill(1);
  for (const b of bonds) {
    const bo = b.bo ?? 1;
    for (const ai of b.atoms) {
      if (ai >= 0 && ai < n) maxBo[ai] = Math.max(maxBo[ai]!, bo);
    }
  }
  const adjUnsat = Array<boolean>(n).fill(false);
  for (const b of bonds) {
    const [a, c] = b.atoms;
    if (a === undefined || c === undefined) continue;
    if (maxBo[a]! >= 1.5) adjUnsat[c] = true;
    if (maxBo[c]! >= 1.5) adjUnsat[a] = true;
  }
  return atoms.map((atom, i) => {
    const z = atom.z ?? 6;
    if (maxBo[i]! >= 1.5) return 2;
    if (z !== 6 && adjUnsat[i]) return 2;
    return 3;
  });
}

function tagMolblockIsotopes(
  molblock: string,
  atomicNums: number[],
  hybCodes: number[]
): string {
  const lines = molblock.replace(/\r\n/g, "\n").split("\n");
  const end = lines.findIndex((l) => l.startsWith("M  END"));
  if (end < 0) return molblock;
  const pairs: string[] = [];
  for (let i = 0; i < atomicNums.length; i++) {
    const iso = atomicNums[i]! * 10 + hybCodes[i]!;
    pairs.push(String(i + 1).padStart(4) + String(iso).padStart(4));
  }
  const chunks: string[] = [];
  for (let i = 0; i < pairs.length; i += 8) {
    const slice = pairs.slice(i, i + 8);
    chunks.push("M  ISO" + String(slice.length).padStart(3) + slice.join(""));
  }
  lines.splice(end, 0, ...chunks);
  return lines.join("\n");
}

function stripMolblockIsotopes(molblock: string): string {
  return molblock
    .replace(/\r\n/g, "\n")
    .split("\n")
    .filter((l) => !l.startsWith("M  ISO"))
    .join("\n");
}

/**
 * Copy with isotopes ``Z×10+hyb`` so MinimalLib ``AtomCompare: Isotopes``
 * matches Python element+hybridization MCS.
 */
export function tagHybridizationIsotopes(
  rdkit: RdkitModule,
  mol: RdkitMol
): RdkitMol | null {
  const json = JSON.parse(mol.get_json()) as RdkitMolJson;
  const atoms = json.molecules[0]?.atoms ?? [];
  const z = atoms.map((a) => a.z ?? 6);
  const hyb = inferHybridizationCodes(json);
  const tagged = rdkit.get_mol(tagMolblockIsotopes(mol.get_molblock(), z, hyb));
  if (!tagged || !tagged.is_valid()) {
    tagged?.delete();
    return null;
  }
  return tagged;
}

/**
 * MinimalLib ``generate_aligned_coords`` details with MCS ``referenceSmarts``.
 * Keep in sync with ``xpict::align_opts::minimallib_align_details``.
 */
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

/** Drop null / negative indices from a MinimalLib match atom list. */
function matchAtomIndices(raw: unknown): number[] {
  if (!Array.isArray(raw)) return [];
  const out: number[] = [];
  for (const x of raw) {
    if (typeof x === "number" && Number.isInteger(x) && x >= 0) out.push(x);
  }
  return out;
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

/**
 * FMCS on hybridization-tagged copies → isotope SMARTS when both tagged mols
 * match it with enough non-null atom indices.
 */
function mcsIsotopeSmarts(
  rdkit: RdkitModule,
  taggedMol: RdkitMol,
  taggedTemplate: RdkitMol
): string | null {
  let qmol: RdkitMol | null = null;
  try {
    const list = new rdkit.MolList();
    list.append(taggedTemplate);
    list.append(taggedMol);
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
    qmol = rdkit.get_qmol(parsed.smarts);
    if (!qmol || !qmol.is_valid()) {
      qmol?.delete();
      return null;
    }
    const tmplHit = JSON.parse(taggedTemplate.get_substruct_match(qmol)) as {
      atoms?: unknown;
    };
    const molHit = JSON.parse(taggedMol.get_substruct_match(qmol)) as {
      atoms?: unknown;
    };
    const ta = matchAtomIndices(tmplHit.atoms);
    const ma = matchAtomIndices(molHit.atoms);
    if (ta.length < MIN_MCS_ATOMS || ta.length !== ma.length) {
      return null;
    }
    return parsed.smarts;
  } finally {
    qmol?.delete();
  }
}

export type LayoutMeta = {
  method: "free" | "atom_map" | "mcs" | "none";
  used_map?: Array<[number, number]>;
};

export type LayoutResult = {
  molecule: MoleculeIn;
  /** Coord-bearing molblock of the laid-out mol — pack into Rendered for align_to. */
  molblock: string;
  meta: LayoutMeta;
};

/** Unique isotopes on mapped atoms (same iso on query+template for each pair). */
function tagExplicitMapIsotopes(
  rdkit: RdkitModule,
  mol: RdkitMol,
  atomMap: Array<[number, number]>,
  side: "query" | "template"
): RdkitMol | null {
  const json = JSON.parse(mol.get_json()) as RdkitMolJson;
  const n = json.molecules[0]?.atoms.length ?? 0;
  const isoByAtom = new Array<number>(n).fill(0);
  atomMap.forEach(([q, t], i) => {
    const idx = side === "query" ? q : t;
    if (idx >= 0 && idx < n) isoByAtom[idx] = 9100 + i;
  });
  const lines = mol.get_molblock().replace(/\r\n/g, "\n").split("\n");
  const end = lines.findIndex((l) => l.startsWith("M  END"));
  if (end < 0) return null;
  const pairs: string[] = [];
  for (let i = 0; i < n; i++) {
    if (isoByAtom[i]! > 0) {
      pairs.push(String(i + 1).padStart(4) + String(isoByAtom[i]!).padStart(4));
    }
  }
  if (!pairs.length) return null;
  const chunks: string[] = [];
  for (let i = 0; i < pairs.length; i += 8) {
    const slice = pairs.slice(i, i + 8);
    chunks.push("M  ISO" + String(slice.length).padStart(3) + slice.join(""));
  }
  lines.splice(end, 0, ...chunks);
  const tagged = rdkit.get_mol(lines.join("\n"));
  if (!tagged || !tagged.is_valid()) {
    tagged?.delete();
    return null;
  }
  return tagged;
}

/**
 * Layout `source` (SMILES / molfile). When `template` is set, RDKit aligns
 * onto that pose (explicit ``atomMap`` or MCS isotope SMARTS).
 */
export async function layoutWithRdkit(
  source: string,
  opts: {
    template?: string | null;
    id?: string;
    /** Pairs [query, template]; skips MCS when set. */
    atomMap?: Array<[number, number]> | null;
    minAtoms?: number;
  } = {}
): Promise<LayoutResult> {
  const rdkit = await ensureRdkit();
  const floor = opts.minAtoms ?? MIN_MCS_ATOMS;
  const mol = getMol(rdkit, source);
  let templateMol: RdkitMol | null = null;
  let taggedMol: RdkitMol | null = null;
  let taggedTemplate: RdkitMol | null = null;
  try {
    if (opts.template) {
      templateMol = getMol(rdkit, opts.template);
      ensureCoords(templateMol);

      let aligned = "";
      let method: LayoutMeta["method"] = "none";
      let used_map: Array<[number, number]> | undefined;

      if (opts.atomMap && opts.atomMap.length >= floor) {
        taggedTemplate = tagExplicitMapIsotopes(
          rdkit,
          templateMol,
          opts.atomMap,
          "template"
        );
        taggedMol = tagExplicitMapIsotopes(rdkit, mol, opts.atomMap, "query");
        if (taggedTemplate && taggedMol) {
          const smarts = mcsIsotopeSmarts(rdkit, taggedMol, taggedTemplate);
          if (smarts) {
            aligned = taggedMol.generate_aligned_coords(
              taggedTemplate,
              minimallibAlignDetails(smarts)
            );
            if (alignSucceeded(aligned) && taggedMol.is_valid()) {
              method = "atom_map";
              used_map = opts.atomMap.map(([q, t]) => [q, t]);
            }
          }
        }
      } else if (!opts.atomMap) {
        taggedTemplate = tagHybridizationIsotopes(rdkit, templateMol);
        taggedMol = tagHybridizationIsotopes(rdkit, mol);
        if (taggedTemplate && taggedMol) {
          const smarts = mcsIsotopeSmarts(rdkit, taggedMol, taggedTemplate);
          if (smarts) {
            aligned = taggedMol.generate_aligned_coords(
              taggedTemplate,
              minimallibAlignDetails(smarts)
            );
            if (alignSucceeded(aligned) && taggedMol.is_valid()) {
              method = "mcs";
            }
          }
        }
      }

      if (method === "none" || !taggedMol?.is_valid()) {
        ensureCoords(mol);
        return {
          molecule: toMoleculeIn(mol, { id: opts.id }),
          molblock: sanitizeDummyMolblock(mol.get_molblock()),
          meta: { method: "none" },
        };
      }

      const tmplJson = JSON.parse(templateMol.get_json()) as RdkitMolJson;
      const tmplBonds = tmplJson.molecules[0]?.bonds ?? [];
      const tmplCoords = parseCoords(templateMol);
      const tmplScale = SCALE / meanBondLength(tmplCoords, tmplBonds);
      let flipMaxY = -Infinity;
      for (const [, y] of tmplCoords) flipMaxY = Math.max(flipMaxY, y);
      return {
        molecule: toMoleculeIn(taggedMol, {
          id: opts.id,
          scale: tmplScale,
          flipMaxY,
        }),
        molblock: sanitizeDummyMolblock(
          stripMolblockIsotopes(taggedMol.get_molblock())
        ),
        meta: { method, used_map },
      };
    }
    ensureCoords(mol);
    return {
      molecule: toMoleculeIn(mol, { id: opts.id }),
      molblock: sanitizeDummyMolblock(mol.get_molblock()),
      meta: { method: "free" },
    };
  } finally {
    mol.delete();
    templateMol?.delete();
    taggedMol?.delete();
    taggedTemplate?.delete();
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
