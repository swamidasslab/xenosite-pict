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
  buildAlignPlan,
  processEdgePlan,
  processEdgePlanWithFrames,
  validateEdgePlan,
  MIN_MCS_ATOMS as EDGE_MIN_MCS_ATOMS,
} from "./edge-plan.js";
export type {
  AlignOpts as EdgeAlignOpts,
  CoordGenMoleculeResult,
  CoordGenTask,
  CoordGenTaskResult,
  EdgePlan,
  EdgeResult,
  MolTemplate,
} from "./edge-plan.js";
