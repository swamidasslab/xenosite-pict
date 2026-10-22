/**
 * Scene types + SVG via Rust ``scene_to_svg`` (wasm).
 *
 * Do not serialize markup here — viewport ``data-id`` and the rest of the
 * document live in ``xpict-core``.
 */

import { sceneToSvgJson } from "../native.js";
import type {
  Layer,
  Primitive,
  Scene,
  Viewport,
} from "../generated/scene-abi.js";

export type {
  Layer,
  LayerName,
  Primitive,
  Scene,
  TextAnchor,
  Viewport,
} from "../generated/scene-abi.js";

/** @deprecated Prefer {@link Primitive}. */
export type ScenePrimitive = Primitive;
/** @deprecated Prefer {@link Layer}. */
export type SceneLayer = Layer;
/** @deprecated Prefer {@link Viewport}. */
export type SceneViewport = Viewport;

/** Serialize a Scene document to SVG (width/height = viewBox = SCALE units). */
export function sceneToSvg(scene: Scene): string {
  return sceneToSvgJson(JSON.stringify(scene));
}
