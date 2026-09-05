"""ELK diagram placement via elkjs (Node) with grid/row fallback."""

from __future__ import annotations

import json
import shutil
import subprocess
import warnings
from collections.abc import Sequence
from pathlib import Path

from xenosite.pict.contracts.layout import MoleculeLayout
from xenosite.pict.contracts.spec import DiagramKind, PictSpec
from xenosite.pict.draw.scene_builder import normalize_coords
from xenosite.pict.warnings import PictBackendWarning

_GAP = 24.0


def _elkjs_script() -> Path | None:
    """Locate js/pocs/elk_layout.mjs (repo checkout or sibling of installed package)."""
    here = Path(__file__).resolve()
    candidates = [
        here.parents[4] / "js" / "pocs" / "elk_layout.mjs",  # .../src/xenosite/pict/diagram
        Path.cwd() / "js" / "pocs" / "elk_layout.mjs",
    ]
    for p in candidates:
        if p.is_file():
            return p
    return None


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


def elk_graph(layouts: Sequence[MoleculeLayout], spec: PictSpec) -> dict:
    nodes = []
    for i, L in enumerate(layouts):
        w, h = normalize_coords(L)[1:]
        nodes.append({"id": L.id or f"m{i}", "width": w, "height": h})
    edges = [
        {"id": f"e{i}", "sources": [e.source], "targets": [e.target]}
        for i, e in enumerate(spec.diagram.edges)
    ]
    return {
        "id": "root",
        "layoutOptions": {
            "elk.algorithm": "layered",
            "elk.direction": "RIGHT",
            "elk.spacing.nodeNode": "40",
            **spec.diagram.elk_options,
        },
        "children": nodes,
        "edges": edges,
    }


def elk_graph_json(layouts: Sequence[MoleculeLayout], spec: PictSpec) -> str:
    return json.dumps(elk_graph(layouts, spec), indent=2)


def _elkjs_positions(
    layouts: Sequence[MoleculeLayout], spec: PictSpec
) -> list[tuple[float, float]] | None:
    """Run elkjs via Node when available (POC bridge)."""
    if not shutil.which("node"):
        return None
    script = _elkjs_script()
    if script is None:
        return None
    graph = elk_graph(layouts, spec)
    try:
        proc = subprocess.run(
            ["node", str(script)],
            input=json.dumps(graph),
            capture_output=True,
            text=True,
            check=True,
            timeout=30,
            cwd=str(script.parent.parent),  # js/ so require("elkjs") resolves
        )
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired, OSError) as exc:
        detail = ""
        if isinstance(exc, subprocess.CalledProcessError) and exc.stderr:
            detail = f": {exc.stderr.strip()[:200]}"
        warnings.warn(
            f"elkjs layout failed ({exc}){detail}; falling back.",
            PictBackendWarning,
            stacklevel=3,
        )
        return None

    laid = json.loads(proc.stdout)
    by_id = {c["id"]: c for c in laid.get("children", [])}
    positions: list[tuple[float, float]] = []
    for i, L in enumerate(layouts):
        nid = L.id or f"m{i}"
        node = by_id.get(nid, {})
        positions.append((float(node.get("x", 0.0)), float(node.get("y", 0.0))))
    return positions


def layout_diagram(
    layouts: Sequence[MoleculeLayout], spec: PictSpec
) -> list[tuple[float, float]]:
    """Return top-left positions for each molecule viewport."""
    if len(layouts) <= 1:
        return [(0.0, 0.0)]

    kind = spec.diagram.kind
    if kind in {DiagramKind.network, DiagramKind.reaction}:
        elk = _elkjs_positions(layouts, spec)
        if elk is not None:
            return elk
        warnings.warn(
            "ELK bridge unavailable; using row layout for network/reaction diagrams.",
            PictBackendWarning,
            stacklevel=3,
        )
        return _row_positions(layouts)

    if kind == DiagramKind.grid:
        cols = spec.diagram.columns or max(1, int(len(layouts) ** 0.5 + 0.5))
        return _grid_positions(layouts, cols)

    return _row_positions(layouts)
