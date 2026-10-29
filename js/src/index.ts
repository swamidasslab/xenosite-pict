/**
 * `@xenosite/xpict` — molecule depiction for the web.
 *
 * Single molecule: ``xpict.mol`` / ``xpict.render`` / ``xpict.toSvg``.
 * ``xpict.depict`` wraps a mol as a spec, or takes a document, and calls render.
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
// Annotation fields (`circle`, `region`, `callout`, …) and their parts.
export type {
  AtomLabelSpec,
  AtomNumberSpec,
  CalloutField,
  CircleField,
  RegionField,
  MolText,
} from "./api.js";
export type { CircleSpec } from "./generated/CircleSpec.js";
export type { RegionSpec } from "./generated/RegionSpec.js";
export type { FillSpec } from "./generated/FillSpec.js";
export type { StrokeSpec } from "./generated/StrokeSpec.js";
