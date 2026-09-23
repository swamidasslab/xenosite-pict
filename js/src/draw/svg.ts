/**
 * SVG drawer stub — same layered scene rules as Python ``draw/svg.py``.
 *
 * Layers: halo (bottom) → shading → bonds → labels → marks → overlay.
 * Halos never obscure drawn ink.
 */

type StubLayout = {
  atoms: unknown[];
  backend: string;
};

export function layoutToSvgStub(layout: StubLayout): string {
  const n = layout.atoms.length;
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" data-xpict="stub" ` +
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
