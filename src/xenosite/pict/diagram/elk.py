"""Multi-molecule diagram placement (ELK when available; grid/row fallback)."""

from __future__ import annotations

import json
import warnings
from collections.abc import Sequence

from xenosite.pict.contracts.layout import MoleculeLayout
from xenosite.pict.contracts.spec import DiagramKind, PictSpec
from xenosite.pict.draw.scene_builder import normalize_coords
from xenosite.pict.warnings import PictBackendWarning

_GAP = 24.0


def _grid_positions(
    layouts: Sequence[MoleculeLayout], columns: int
) -> list[tuple[float, float]]:
    cols = max(1, columns)
    sizes = [normalize_coords(L)[1:] for L in layouts]
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


def _row_positions(layouts: Sequence[MoleculeLayout]) -> list[tuple[float, float]]:
    x = 0.0
    positions: list[tuple[float, float]] = []
    for L in layouts:
        w, _h = normalize_coords(L)[1:]
        positions.append((x, 0.0))
        x += w + _GAP
    return positions


def layout_diagram(
    layouts: Sequence[MoleculeLayout], spec: PictSpec
) -> list[tuple[float, float]]:
    """Return top-left positions for each molecule viewport."""
    if len(layouts) <= 1:
        return [(0.0, 0.0)]

    kind = spec.diagram.kind
    if kind in {DiagramKind.network, DiagramKind.reaction}:
        # ELK JAR+V8 / elkjs bridge not wired yet in this scaffold.
        warnings.warn(
            "ELK bridge not available yet; using row layout for network/reaction diagrams.",
            PictBackendWarning,
            stacklevel=3,
        )
        return _row_positions(layouts)

    if kind == DiagramKind.grid:
        cols = spec.diagram.columns or max(1, int(len(layouts) ** 0.5 + 0.5))
        return _grid_positions(layouts, cols)

    return _row_positions(layouts)


def elk_graph_json(layouts: Sequence[MoleculeLayout], spec: PictSpec) -> str:
    """ELK JSON graph we will send to the bridge once wired."""
    nodes = []
    for i, L in enumerate(layouts):
        w, h = normalize_coords(L)[1:]
        nodes.append({"id": L.id or f"m{i}", "width": w, "height": h})
    edges = [
        {"id": f"e{i}", "sources": [e.source], "targets": [e.target]}
        for i, e in enumerate(spec.diagram.edges)
    ]
    graph = {
        "id": "root",
        "layoutOptions": {"elk.algorithm": "layered", **spec.diagram.elk_options},
        "children": nodes,
        "edges": edges,
    }
    return json.dumps(graph, indent=2)
