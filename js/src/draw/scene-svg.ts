/**
 * Scene JSON → SVG string (thin serializer matching Python ``scene_to_svg``).
 *
 * Text primitives emit ``<text>`` for now (glyph outlining stays Python/Rust).
 */

export type TextAnchor = "start" | "middle" | "end";

export type ScenePrimitive =
  | {
      kind: "path";
      d: string;
      stroke?: string | null;
      fill?: string | null;
      stroke_width?: number;
      opacity?: number;
      stroke_dasharray?: string | null;
      stroke_linecap?: string | null;
      cls?: string | null;
    }
  | {
      kind: "circle";
      cx: number;
      cy: number;
      r: number;
      fill?: string | null;
      stroke?: string | null;
      stroke_width?: number;
      opacity?: number;
      cls?: string | null;
    }
  | {
      kind: "text";
      x: number;
      y: number;
      text: string;
      fill?: string;
      font_size?: number;
      anchor?: TextAnchor;
      cls?: string | null;
    };

export type SceneLayer = {
  name: string;
  primitives: ScenePrimitive[];
};

export type SceneViewport = {
  id?: string | null;
  x: number;
  y: number;
  width: number;
  height: number;
  layers: SceneLayer[];
};

export type Scene = {
  width: number;
  height: number;
  viewports: SceneViewport[];
  overlays?: ScenePrimitive[];
  halo?: ScenePrimitive[];
};

function esc(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/"/g, "&quot;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function fmt(n: number): string {
  if (!Number.isFinite(n)) return "0";
  const t = Math.round(n * 1e4) / 1e4;
  return String(t);
}

function attr(name: string, value: string | number | null | undefined): string {
  if (value === null || value === undefined || value === "") return "";
  const v = typeof value === "number" ? fmt(value) : String(value);
  return ` ${name}="${esc(v)}"`;
}

function renderPrimitive(p: ScenePrimitive): string {
  if (p.kind === "path") {
    return (
      `<path` +
      attr("d", p.d) +
      attr("fill", p.fill ?? "none") +
      attr("stroke", p.stroke ?? "none") +
      attr("stroke-width", p.stroke_width ?? 1.5) +
      attr("stroke-linecap", p.stroke_linecap ?? "round") +
      attr("stroke-linejoin", "round") +
      attr("opacity", p.opacity ?? 1) +
      attr("stroke-dasharray", p.stroke_dasharray) +
      attr("class", p.cls) +
      `/>`
    );
  }
  if (p.kind === "circle") {
    return (
      `<circle` +
      attr("cx", fmt(p.cx)) +
      attr("cy", fmt(p.cy)) +
      attr("r", fmt(p.r)) +
      attr("fill", p.fill ?? "none") +
      attr("stroke", p.stroke) +
      attr("stroke-width", p.stroke !== undefined ? p.stroke_width ?? 1.5 : undefined) +
      attr("opacity", p.opacity ?? 1) +
      attr("class", p.cls) +
      `/>`
    );
  }
  // text — host may outline later; `<text>` keeps positions honest for MVP
  return (
    `<text` +
    attr("x", fmt(p.x)) +
    attr("y", fmt(p.y)) +
    attr("fill", p.fill ?? "#000") +
    attr("font-size", p.font_size ?? 12) +
    attr("font-family", "Liberation Sans, Arial, sans-serif") +
    attr("text-anchor", p.anchor ?? "middle") +
    attr("dominant-baseline", "alphabetic") +
    attr("class", p.cls) +
    `>${esc(p.text)}</text>`
  );
}

function renderViewportLayers(
  vp: SceneViewport,
  names: string[]
): string {
  const want = new Set(names);
  const chunks: string[] = [];
  for (const layer of vp.layers) {
    if (!want.has(layer.name) || !layer.primitives.length) continue;
    const body = layer.primitives.map(renderPrimitive).join("");
    chunks.push(`<g class="xpict-layer xpict-${esc(layer.name)}">${body}</g>`);
  }
  return chunks.join("");
}

/** Serialize a Scene document to SVG (width/height = viewBox = SCALE units). */
export function sceneToSvg(scene: Scene): string {
  const w = fmt(scene.width);
  const h = fmt(scene.height);
  const parts: string[] = [];
  for (const vp of scene.viewports) {
    parts.push(renderViewportLayers(vp, ["shading"]));
  }
  if (scene.halo?.length) {
    parts.push(
      `<g class="xpict-halo" id="halo">${scene.halo.map(renderPrimitive).join("")}</g>`
    );
  }
  for (const vp of scene.viewports) {
    parts.push(renderViewportLayers(vp, ["bonds", "labels", "marks", "overlay"]));
  }
  if (scene.overlays?.length) {
    parts.push(
      `<g class="xpict-overlays" id="overlays">${scene.overlays
        .map(renderPrimitive)
        .join("")}</g>`
    );
  }
  return (
    `<?xml version="1.0" encoding="UTF-8"?>\n` +
    `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" ` +
    `viewBox="0 0 ${w} ${h}" class="xpict">${parts.join("")}</svg>`
  );
}
