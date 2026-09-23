/**
 * `@xenosite/xpict` — molecule depiction for the web.
 *
 * Single molecule: ``xpict.mol`` / ``xpict.render`` / ``xpict.toSvg``.
 * Declarative document: ``xpict.depict`` (nested PictSpec subset).
 */

export { xpict } from "./api.js";
export type {
  Mol,
  Rendered,
  AlignTarget,
  MolRenderOptions,
  MolNode,
  GroupNode,
  MolSpec,
  DepictSpec,
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
