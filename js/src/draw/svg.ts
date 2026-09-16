/**
 * SVG drawer stub — same layered scene rules as Python ``draw/svg.py``.
 *
 * Layers (xenopict-inspired): shading → halo → bonds → labels → marks → overlay.
 * Layout backends must not supply SVG; only coords + topology.
 */

import type { MoleculeLayout } from "../types.js";

export function layoutToSvgStub(layout: MoleculeLayout): string {
  const n = layout.atoms.length;
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" data-xenosite-pict="stub" ` +
    `data-backend="${escapeXml(layout.backend)}" data-atoms="${n}">` +
    `<text x="8" y="20" font-size="12">SVG drawer stub (${n} atoms)</text>` +
    `</svg>`
  );
}

function escapeXml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/"/g, "&quot;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}
