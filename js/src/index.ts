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
  ReactionSchemeNode,
  MolSpec,
  DepictSpec,
  AlignTo,
  AlignToSpec,
  ShadeSpec,
  ShadeStyle,
  CommonOpts,
  MolOpts,
  NodeType,
  ForTypesPatch,
  TypedOptsPatch,
  OptsPatch,
  Opts,
  EdgeArrow,
  EdgeNode,
  TextNode,
  LabelPos,
  LabelPlacement,
  LabelItem,
  LabelLanes,
  Label,
  MolIds,
  LayoutDirection,
  EdgeRouting,
  LayoutAlgorithm,
  LayoutOpts,
  Node,
  SvgAtom,
  SvgBond,
  Scene,
  ScenePrimitive,
  SceneLayer,
  SceneViewport,
  Primitive,
  Layer,
  Viewport,
  LayerName,
  TextAnchor,
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
