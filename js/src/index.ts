/**
 * `@xenosite/xpict` — molecule depiction for xenosite.org.
 *
 * Preferred: ``xpict.depict`` (nested PictSpec subset).
 * Simple: ``xpict.mol`` / ``xpict.render`` / ``xpict.toSvg``.
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
