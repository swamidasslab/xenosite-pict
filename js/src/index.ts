/**
 * `@swamidasslab/xpict` — MVP client for xenosite.org.
 *
 * Public: ``xpict.mol`` / ``xpict.render`` / ``xpict.toSvg`` (+ types).
 */

export { xpict } from "./api.js";
export type {
  Mol,
  Rendered,
  AlignTarget,
  MolRenderOptions,
  SvgAtom,
  SvgBond,
  Scene,
  ScenePrimitive,
  SceneLayer,
  SceneViewport,
} from "./api.js";
export {
  cxAtomLabels,
  smilesBase,
  smilesStarAtomIndices,
  starLabelsFromCxsmiles,
} from "./cxsmiles.js";
