/**
 * Pad / translate MoleculeIn coords the same way Rust ``depict_molecule`` does,
 * so returned atom positions match SVG viewBox ink.
 */

import type { MoleculeIn } from "./layout/rdkit-layout.js";
import { SCALE } from "./layout/rdkit-layout.js";

/** Matches Rust ``PAD_PX`` / ``MARK_FRAC`` (SCALE-space). */
const PAD_PX = SCALE;
const MARK_FRAC = 1.0;

export type SvgAtom = {
  index: number;
  element: string;
  x: number;
  y: number;
  label?: string;
  charge?: number;
};

export type SvgBond = {
  index: number;
  begin: number;
  end: number;
  order: number;
  stereo?: string;
};

export type FrameTransform = {
  dx: number;
  dy: number;
  width: number;
  height: number;
};

/** Same bbox + pad math as ``xpict_core::depict::depict_molecule``. */
export function frameTransform(mol: MoleculeIn): FrameTransform {
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const a of mol.atoms) {
    minX = Math.min(minX, a.x);
    minY = Math.min(minY, a.y);
    maxX = Math.max(maxX, a.x);
    maxY = Math.max(maxY, a.y);
  }
  if (!mol.atoms.length) {
    minX = 0;
    minY = 0;
    maxX = 0;
    maxY = 0;
  } else if (mol.mark_atoms?.length) {
    const markR = SCALE * MARK_FRAC;
    minX -= markR;
    minY -= markR;
    maxX += markR;
    maxY += markR;
  }
  const pad = PAD_PX;
  return {
    dx: pad - minX,
    dy: pad - minY,
    width: maxX - minX + 2 * pad,
    height: maxY - minY + 2 * pad,
  };
}

/** Atom centers in SVG viewBox space (exactly where depict paints them). */
export function atomsInSvgFrame(mol: MoleculeIn): {
  atoms: SvgAtom[];
  bonds: SvgBond[];
  width: number;
  height: number;
  dx: number;
  dy: number;
} {
  const { dx, dy, width, height } = frameTransform(mol);
  const atoms: SvgAtom[] = mol.atoms.map((a) => ({
    index: a.index,
    element: a.element,
    x: a.x + dx,
    y: a.y + dy,
    ...(a.label ? { label: a.label } : {}),
    ...(a.charge ? { charge: a.charge } : {}),
  }));
  const bonds: SvgBond[] = mol.bonds.map((b) => ({
    index: b.index,
    begin: b.begin,
    end: b.end,
    order: b.order,
    ...(b.stereo ? { stereo: b.stereo } : {}),
  }));
  return { atoms, bonds, width, height, dx, dy };
}
