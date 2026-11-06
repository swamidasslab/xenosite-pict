/**
 * SVG medium defaults for host export formats.
 *
 * Rust paints `svg: "web"` (feMorphology) by default. PNG/PDF pipelines often
 * drop SVG filters, so hosts inject `svg: "print"` (path halos) when the author
 * did not set `svg`. Inline / review SVG stays web.
 */

/** Host output formats that should prefer print-safe SVG when `svg` is unset. */
export const PRINT_SVG_FORMATS = new Set(["png", "pdf"]);

/**
 * Return a DepictSpec with `svg: "print"` when exporting PNG/PDF.
 * No-op when `format` is not png/pdf, or when the root already sets `svg`.
 */
export function applyOutputSvgDefault<T extends Record<string, unknown>>(
  live: T,
  format: string
): T {
  if (!PRINT_SVG_FORMATS.has(format)) return live;
  if (live.svg != null) return live;
  return { ...live, svg: "print" };
}
