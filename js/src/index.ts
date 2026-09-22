/**
 * `@swamidasslab/xpict` — MVP client for xenosite.org.
 *
 * Public surface is the ``xpict`` namespace only (JSON shapes, no classes).
 * Low-level wasm helpers remain available via ``@swamidasslab/xpict/native``.
 */

export { xpict } from "./api.js";
export type {
  InitOptions,
  Mol,
  Rendered,
  AlignTarget,
  MolRenderOptions,
  SvgAtom,
  SvgBond,
} from "./api.js";
