"""Reaction / network edge arrows — thin host over ``xpict-core::arrows``."""

from __future__ import annotations

import json
from collections.abc import Sequence

from xpict import _native as _rust
from xpict.contracts.scene import PathPrim, Primitive, TextPrim, Viewport
from xpict.future.spec import EdgeSpec

_GAP = 6.0


def _vp_center(vp: Viewport) -> tuple[float, float]:
    return vp.x + vp.width * 0.5, vp.y + vp.height * 0.5


def _clip_box_edge(
    cx: float, cy: float, tx: float, ty: float, w: float, h: float, pad: float
) -> tuple[float, float]:
    """Point on the padded axis-aligned box around (cx,cy) toward (tx,ty)."""
    dx, dy = tx - cx, ty - cy
    if abs(dx) < 1e-9 and abs(dy) < 1e-9:
        return cx, cy
    hw, hh = w * 0.5 + pad, h * 0.5 + pad
    sx = hw / abs(dx) if abs(dx) > 1e-9 else float("inf")
    sy = hh / abs(dy) if abs(dy) > 1e-9 else float("inf")
    t = min(sx, sy)
    return cx + dx * t, cy + dy * t


def edge_anchors(
    src: Viewport, tgt: Viewport, *, pad: float = _GAP
) -> tuple[tuple[float, float], tuple[float, float]]:
    """Viewport-boundary anchors for an edge from ``src`` to ``tgt``."""
    sx, sy = _vp_center(src)
    tx, ty = _vp_center(tgt)
    p0 = _clip_box_edge(sx, sy, tx, ty, src.width, src.height, pad)
    p1 = _clip_box_edge(tx, ty, sx, sy, tgt.width, tgt.height, pad)
    return p0, p1


def simplify_route(
    pts: Sequence[tuple[float, float]],
    *,
    kink_px: float | None = None,
) -> list[tuple[float, float]]:
    """Drop near-duplicates and flatten kinks (Rust)."""
    raw = _rust.simplify_route(
        json.dumps([[float(x), float(y)] for x, y in pts]), kink_px
    )
    return [(float(x), float(y)) for x, y in json.loads(raw)]


def filleted_path_d(
    pts: Sequence[tuple[float, float]],
    *,
    radius: float | None = None,
) -> str:
    """Polyline with quadratic fillets (Rust)."""
    return str(
        _rust.filleted_path_d(
            json.dumps([[float(x), float(y)] for x, y in pts]), radius
        )
    )


def resolve_route(
    src: Viewport,
    tgt: Viewport,
    route: Sequence[tuple[float, float]] | None = None,
) -> list[tuple[float, float]]:
    """ELK polyline when present; otherwise straight viewport-boundary anchors."""
    if route is not None and len(route) >= 2:
        return simplify_route([(float(x), float(y)) for x, y in route])
    (x1, y1), (x2, y2) = edge_anchors(src, tgt)
    return [(x1, y1), (x2, y2)]


def _name(val: object | None) -> str | None:
    if val is None:
        return None
    return str(getattr(val, "value", val)).lower()


def _from_native_prims(raw: list[dict]) -> list[Primitive]:
    out: list[Primitive] = []
    for p in raw:
        kind = p.get("kind")
        if kind == "path":
            out.append(
                PathPrim(
                    d=p["d"],
                    stroke=p.get("stroke"),
                    fill=p.get("fill"),
                    stroke_width=float(p.get("stroke_width") or 0.0),
                    stroke_dasharray=p.get("stroke_dasharray"),
                    cls=p.get("cls"),
                )
            )
        elif kind == "text":
            out.append(
                TextPrim(
                    x=float(p["x"]),
                    y=float(p["y"]),
                    text=p["text"],
                    fill=p.get("fill") or "#222",
                    font_size=float(p.get("font_size") or 11.0),
                    anchor=p.get("anchor") or "middle",
                    cls=p.get("cls"),
                )
            )
    return out


def edge_primitives(
    edge: EdgeSpec,
    src: Viewport,
    tgt: Viewport,
    *,
    index: int = 0,
    route: Sequence[tuple[float, float]] | None = None,
    scheme_routing: object | None = None,
) -> list[Primitive]:
    """Build document-space primitives for one diagram edge (Rust paint)."""
    pts = resolve_route(src, tgt, route)
    routing = _name(getattr(edge, "edge_routing", None) or scheme_routing)
    payload = {
        "pts": [[float(x), float(y)] for x, y in pts],
        "arrow": _name(edge.arrow) or "forward",
        "routing": routing,
        "color": edge.color,
        "stroke_width": edge.stroke_width,
        "dashed": bool(edge.dashed),
        "label": edge.label,
        "label_pos": getattr(edge, "label_pos", None),
        "index": index,
    }
    return _from_native_prims(
        json.loads(_rust.edge_overlay_primitives(json.dumps(payload)))
    )


def diagram_overlays(
    edges: Sequence[EdgeSpec],
    viewports: Sequence[Viewport],
    edge_paths: Sequence[Sequence[tuple[float, float]] | None] | None = None,
    *,
    scheme_routing: object | None = None,
) -> list[Primitive]:
    """Document overlays for all edges that resolve to placed viewports."""
    by_id = {vp.id: vp for vp in viewports if vp.id}
    prims: list[Primitive] = []
    for i, edge in enumerate(edges):
        src = by_id.get(edge.sources[0]) if edge.sources else None
        tgt = by_id.get(edge.targets[0]) if edge.targets else None
        if src is None or tgt is None:
            continue
        route = None
        if edge_paths is not None and i < len(edge_paths):
            route = edge_paths[i]
        prims.extend(
            edge_primitives(
                edge,
                src,
                tgt,
                index=i,
                route=route,
                scheme_routing=scheme_routing,
            )
        )
    return prims
