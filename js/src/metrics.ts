/**
 * Depiction metrics — keep in sync with `xpict-core` / `xpict.draw.metrics`.
 *
 * Scene `stroke_width` is in **stem units** (1 = default bond ink). Scene JSON
 * keeps those units; the SVG renderer multiplies by [`STROKE_PX`].
 */

/** Bond length in drawing / CSS-px units. */
export const BOND_PX = 20;

/** Atom-label em / bond (optical match to RDKit MolDraw2D). */
export const FONT_FRAC = 0.45;

/** Liberation Sans Regular vertical stem width in em. */
export const FONT_STEM_EM = 0.0933;

/** Bond stroke as a fraction of bond length (stem × font). */
export const STROKE_FRAC =
  Math.round(FONT_STEM_EM * FONT_FRAC * 1000) / 1000;

/** Absolute bond ink width in drawing px (= one stem unit). */
export const STROKE_PX = STROKE_FRAC * BOND_PX;

/** Scene stroke_width (stem units) → SVG px. */
export function strokeWidthPx(stemUnits: number | undefined | null): number {
  if (stemUnits === undefined || stemUnits === null) return STROKE_PX;
  return stemUnits * STROKE_PX;
}
