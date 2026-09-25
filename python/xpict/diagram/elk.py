"""ELK diagram placement via native elkrs (Rust), with grid/row fallback.

Requires ``xpict._native`` (maturin). Grid/row stay pure-Python when ELK is
unavailable or fails. See ``docs/dev/layout-notes.md``.
"""

from __future__ import annotations

import json
import warnings
from collections.abc import Sequence
from dataclasses import dataclass, field
from typing import Any

from xpict.contracts.layout import MoleculeLayout
from xpict.future.spec import DiagramKind, LegacyPictSpec, MoleculeSpec
from xpict.draw.metrics import shared_coord_scale
from xpict.draw.scene_builder import viewport_size
from xpict.draw.text_metrics import measure_text
from xpict.warnings import PictBackendWarning

# Match ``arrows.edge_primitives`` edge-label font so ELK reserves the right box.
_EDGE_LABEL_FONT_PX = 11.0
_EDGE_LABEL_PAD = 4.0


def _flat(spec: LegacyPictSpec | object) -> LegacyPictSpec:
    """Accept nested PictSpec (has ``to_legacy``) or an already-flat document."""
    to_legacy = getattr(spec, "to_legacy", None)
    if callable(to_legacy):
        return to_legacy()  # type: ignore[no-any-return]
    return spec  # type: ignore[return-value]


_GAP = 24.0
_REACTION_GAP = 56.0  # room for arrow shafts + edge labels between molecules


def _label_size(text: str, *, font_px: float = _EDGE_LABEL_FONT_PX) -> tuple[float, float]:
    """Measured width/height for a layout-engine label box."""
    m = measure_text(text, font_px)
    w = max(m.advance, m.ink.width if m.ink is not None else 0.0) + _EDGE_LABEL_PAD
    h = m.typo.height + _EDGE_LABEL_PAD
    return w, h


def _elk_edge_label_side(pos: str | None) -> str:
    """Map above/below/left/right → ELK layered ``sideSelection``."""
    p = (pos or "above").lower()
    if p in {"below", "right"}:
        return "ALWAYS_DOWN"
    return "ALWAYS_UP"


def _elk_edge_labels(edge: Any, edge_id: str) -> list[dict[str, Any]]:
    """Build ELK edge ``labels`` with measured boxes + placement."""
    text = getattr(edge, "label", None)
    if not text or not str(text).strip():
        return []
    raw = str(text).strip()
    w, h = _label_size(raw)
    pos = getattr(edge, "label_pos", None) or "above"
    return [
        {
            "id": f"{edge_id}_lab",
            "text": raw,
            "width": w,
            "height": h,
            "layoutOptions": {
                "elk.edgeLabels.placement": "CENTER",
                "elk.layered.edgeLabels.sideSelection": _elk_edge_label_side(
                    pos if isinstance(pos, str) else getattr(pos, "value", "above")
                ),
            },
        }
    ]


def _viewport_sizes(
    layouts: Sequence[MoleculeLayout], spec: LegacyPictSpec
) -> list[tuple[float, float]]:
    scale = shared_coord_scale(layouts)
    sizes: list[tuple[float, float]] = []
    for i, layout in enumerate(layouts):
        mol: MoleculeSpec | None = spec.molecules[i] if i < len(spec.molecules) else None
        sizes.append(viewport_size(layout, mol, scale=scale))
    return sizes


@dataclass
class DiagramPlacement:
    """Node positions plus optional ELK edge routes (document space)."""

    positions: list[tuple[float, float]]
    edge_paths: list[list[tuple[float, float]] | None] = field(default_factory=list)
    width: float | None = None
    height: float | None = None


def _grid_positions(
    layouts: Sequence[MoleculeLayout],
    columns: int,
    sizes: Sequence[tuple[float, float]],
) -> list[tuple[float, float]]:
    cols = max(1, columns)
    n = len(layouts)
    rows = (n + cols - 1) // cols
    col_w = [0.0] * cols
    row_h = [0.0] * rows
    for i, (w, h) in enumerate(sizes):
        r, c = divmod(i, cols)
        col_w[c] = max(col_w[c], w)
        row_h[r] = max(row_h[r], h)
    xs = [0.0]
    for c in range(cols - 1):
        xs.append(xs[-1] + col_w[c] + _GAP)
    ys = [0.0]
    for r in range(rows - 1):
        ys.append(ys[-1] + row_h[r] + _GAP)
    return [(xs[c], ys[r]) for i in range(n) for r, c in [divmod(i, cols)]]


def _row_positions(
    sizes: Sequence[tuple[float, float]], *, gap: float = _GAP, center_y: bool = False
) -> list[tuple[float, float]]:
    max_h = max((h for _w, h in sizes), default=0.0)
    x = 0.0
    positions: list[tuple[float, float]] = []
    for w, h in sizes:
        y = (max_h - h) * 0.5 if center_y else 0.0
        positions.append((x, y))
        x += w + gap
    return positions


def _reaction_defaults(spec: LegacyPictSpec) -> dict[str, str]:
    """ELK defaults for network / reaction diagrams (caller options win)."""
    base = {
        "elk.algorithm": "layered",
        "elk.direction": "RIGHT",
        "elk.edgeRouting": "POLYLINE",
        "elk.spacing.nodeNode": "40",
        "elk.spacing.edgeEdge": "16",
        "elk.spacing.edgeNode": "20",
        "elk.layered.spacing.nodeNodeBetweenLayers": "48",
        "elk.layered.spacing.edgeNodeBetweenLayers": "24",
        "elk.layered.crossingMinimization.strategy": "LAYER_SWEEP",
        "elk.layered.nodePlacement.strategy": "NETWORK_SIMPLEX",
    }
    if spec.diagram.kind == DiagramKind.reaction:
        base.update(
            {
                "elk.spacing.nodeNode": "56",
                "elk.layered.spacing.nodeNodeBetweenLayers": "80",
                "elk.layered.spacing.edgeNodeBetweenLayers": "28",
                "elk.spacing.edgeEdge": "20",
                # Prefer spreading branches so metabolite sinks don't stack.
                "elk.layered.crossingMinimization.forceNodeModelOrder": "false",
            }
        )
    # Generic packing knobs (backend-agnostic names on DiagramSpec).
    if spec.diagram.node_spacing is not None:
        base["elk.spacing.nodeNode"] = str(spec.diagram.node_spacing)
    if spec.diagram.layer_spacing is not None:
        base["elk.layered.spacing.nodeNodeBetweenLayers"] = str(
            spec.diagram.layer_spacing
        )
        # Keep edge–node clearance proportional when packing changes.
        base["elk.layered.spacing.edgeNodeBetweenLayers"] = str(
            max(12.0, float(spec.diagram.layer_spacing) * 0.35)
        )
    return base


def elk_graph(layouts: Sequence[MoleculeLayout], spec: LegacyPictSpec | object) -> dict[str, Any]:
    spec = _flat(spec)
    sizes = _viewport_sizes(layouts, spec)
    nodes = []
    for i, L in enumerate(layouts):
        w, h = sizes[i]
        nodes.append({"id": L.id or f"m{i}", "width": w, "height": h})
    edges = []
    for i, e in enumerate(spec.diagram.edges):
        eid = f"e{i}"
        entry: dict[str, Any] = {
            "id": eid,
            "sources": list(e.sources),
            "targets": list(e.targets),
        }
        labels = _elk_edge_labels(e, eid)
        if labels:
            entry["labels"] = labels
        # Per-edge routing override when set (host may also map EdgeNode.edge_routing).
        routing = getattr(e, "edge_routing", None)
        if routing is not None:
            entry["layoutOptions"] = {
                "elk.edgeRouting": str(
                    routing.value if hasattr(routing, "value") else routing
                ).upper()
            }
        edges.append(entry)
    return {
        "id": "root",
        "layoutOptions": {
            **_reaction_defaults(spec),
            **{str(k): str(v) for k, v in spec.diagram.elk_options.items()},
        },
        "children": nodes,
        "edges": edges,
    }


def elk_graph_json(layouts: Sequence[MoleculeLayout], spec: LegacyPictSpec) -> str:
    return json.dumps(elk_graph(layouts, spec), indent=2)


def _section_points(section: dict[str, Any]) -> list[tuple[float, float]]:
    """Flatten one ELK edge section into a polyline (start → bends → end)."""
    start = section.get("startPoint") or {}
    end = section.get("endPoint") or {}
    pts: list[tuple[float, float]] = [(float(start.get("x", 0.0)), float(start.get("y", 0.0)))]
    for bp in section.get("bendPoints") or []:
        pts.append((float(bp.get("x", 0.0)), float(bp.get("y", 0.0))))
    pts.append((float(end.get("x", 0.0)), float(end.get("y", 0.0))))
    # Drop consecutive duplicates (ELK sometimes emits zero-length stubs).
    cleaned: list[tuple[float, float]] = []
    for p in pts:
        if not cleaned or abs(p[0] - cleaned[-1][0]) > 1e-6 or abs(p[1] - cleaned[-1][1]) > 1e-6:
            cleaned.append(p)
    return cleaned


def _edge_path_from_elk(edge: dict[str, Any]) -> list[tuple[float, float]] | None:
    sections = edge.get("sections") or []
    if not sections:
        return None
    path: list[tuple[float, float]] = []
    for sec in sections:
        pts = _section_points(sec)
        if not pts:
            continue
        if not path:
            path.extend(pts)
        else:
            # Join sections; skip duplicated junction.
            path.extend(pts[1:] if pts[0] == path[-1] else pts)
    return path if len(path) >= 2 else None


def _placement_from_laid(
    layouts: Sequence[MoleculeLayout], spec: LegacyPictSpec, laid: dict[str, Any]
) -> DiagramPlacement:
    by_id = {c["id"]: c for c in laid.get("children", [])}
    positions: list[tuple[float, float]] = []
    for i, L in enumerate(layouts):
        nid = L.id or f"m{i}"
        node = by_id.get(nid, {})
        positions.append((float(node.get("x", 0.0)), float(node.get("y", 0.0))))

    by_edge = {e.get("id"): e for e in laid.get("edges", [])}
    edge_paths: list[list[tuple[float, float]] | None] = []
    for i, _e in enumerate(spec.diagram.edges):
        raw = by_edge.get(f"e{i}")
        edge_paths.append(_edge_path_from_elk(raw) if raw else None)

    w = laid.get("width")
    h = laid.get("height")
    return DiagramPlacement(
        positions=positions,
        edge_paths=edge_paths,
        width=float(w) if w is not None else None,
        height=float(h) if h is not None else None,
    )


def _elkrs_placement(
    layouts: Sequence[MoleculeLayout], spec: LegacyPictSpec
) -> DiagramPlacement | None:
    """Native elkrs via ``xpict._native``."""
    try:
        from xpict import _native
    except ImportError:
        return None
    layout_fn = getattr(_native, "elk_layout_json", None)
    if layout_fn is None:
        return None
    graph = elk_graph(layouts, spec)
    try:
        laid = json.loads(layout_fn(json.dumps(graph)))
    except Exception as exc:
        warnings.warn(
            f"elkrs layout failed ({exc}); falling back to row layout.",
            PictBackendWarning,
            stacklevel=3,
        )
        return None
    return _placement_from_laid(layouts, spec, laid)


def layout_diagram_ex(
    layouts: Sequence[MoleculeLayout], spec: LegacyPictSpec | object
) -> DiagramPlacement:
    """Place molecule viewports; for network/reaction also return ELK edge routes."""
    spec = _flat(spec)
    if len(layouts) <= 1:
        return DiagramPlacement(positions=[(0.0, 0.0)], edge_paths=[])

    sizes = _viewport_sizes(layouts, spec)
    kind = spec.diagram.kind
    if kind in {DiagramKind.network, DiagramKind.reaction}:
        elk = _elkrs_placement(layouts, spec)
        if elk is not None:
            return elk
        warnings.warn(
            "ELK (elkrs) unavailable; using row layout for network/reaction diagrams.",
            PictBackendWarning,
            stacklevel=3,
        )
        if kind == DiagramKind.reaction:
            pos = _row_positions(sizes, gap=_REACTION_GAP, center_y=True)
        else:
            pos = _row_positions(sizes)
        return DiagramPlacement(positions=pos, edge_paths=[])

    if kind == DiagramKind.grid:
        cols = spec.diagram.columns or max(1, int(len(layouts) ** 0.5 + 0.5))
        return DiagramPlacement(positions=_grid_positions(layouts, cols, sizes), edge_paths=[])

    return DiagramPlacement(positions=_row_positions(sizes), edge_paths=[])


def layout_diagram(
    layouts: Sequence[MoleculeLayout], spec: LegacyPictSpec | object
) -> list[tuple[float, float]]:
    """Return top-left positions for each molecule viewport."""
    return layout_diagram_ex(layouts, spec).positions
