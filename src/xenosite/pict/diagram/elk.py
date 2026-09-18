"""ELK diagram placement via elkjs in jsrun (embedded V8), with grid/row fallback.

No Node subprocess. Vendored ``elk-api.js`` + ``elk-worker.min.js`` load into a
cached jsrun Runtime. See ``docs/layout-notes.md``.
"""

from __future__ import annotations

import asyncio
import json
import threading
import warnings
from collections.abc import Sequence
from dataclasses import dataclass, field
from pathlib import Path

from xenosite.pict.contracts.layout import MoleculeLayout
from xenosite.pict.contracts.spec import DiagramKind, PictSpec
from xenosite.pict.draw.scene_builder import normalize_coords
from xenosite.pict.warnings import PictBackendWarning

_GAP = 24.0
_REACTION_GAP = 56.0  # room for arrow shafts + edge labels between molecules
_VENDOR = Path(__file__).resolve().parents[1] / "vendor" / "elkjs"

_runtime_lock = threading.Lock()
_runtime = None  # jsrun.Runtime | None
_elk_ready = False


@dataclass
class DiagramPlacement:
    """Node positions plus optional ELK edge routes (document space)."""

    positions: list[tuple[float, float]]
    edge_paths: list[list[tuple[float, float]] | None] = field(default_factory=list)
    width: float | None = None
    height: float | None = None


def _polyfills() -> str:
    return """
    (function () {
      // GWT/elk-worker resolves $wnd via window | global | self.
      // Provide Node-like `global` only — do NOT define `self`, or
      // elk-worker.min.js takes the Web-Worker branch and skips exports.
      if (typeof globalThis.global === "undefined") {
        globalThis.global = globalThis;
      }
      try { Error.stackTraceLimit = 0; } catch (e) {}
      let n = 0;
      const pending = new Map();
      globalThis.setTimeout = function (cb, _ms) {
        const id = ++n;
        pending.set(id, cb);
        Promise.resolve().then(() => {
          const fn = pending.get(id);
          if (fn) {
            pending.delete(id);
            fn();
          }
        });
        return id;
      };
      globalThis.clearTimeout = function (id) {
        pending.delete(id);
      };
    })();
    """


def _ensure_elk_runtime():
    """Load vendored elkjs into a process-wide jsrun Runtime (lazy, thread-safe)."""
    global _runtime, _elk_ready
    if _elk_ready and _runtime is not None:
        return _runtime
    with _runtime_lock:
        if _elk_ready and _runtime is not None:
            return _runtime
        try:
            from jsrun import Runtime
        except ImportError as e:
            raise ImportError(
                "ELK layout requires jsrun. It is a core dependency of xenosite-pict."
            ) from e

        api_path = _VENDOR / "elk-api.js"
        worker_path = _VENDOR / "elk-worker.min.js"
        if not api_path.is_file() or not worker_path.is_file():
            raise FileNotFoundError(
                f"Vendored elkjs assets missing under {_VENDOR}. "
                "Expected elk-api.js and elk-worker.min.js."
            )

        rt = Runtime()
        rt.eval(_polyfills())
        worker_src = worker_path.read_text(encoding="utf-8")
        api_src = api_path.read_text(encoding="utf-8")
        rt.eval(
            "(function(){\n"
            "  var module = { exports: {} };\n"
            "  var exports = module.exports;\n"
            f"{worker_src}\n"
            "  globalThis.__ElkWorkerMod__ = module.exports;\n"
            "})();"
        )
        rt.eval(
            "(function(){\n"
            "  var module = { exports: {} };\n"
            "  var exports = module.exports;\n"
            "  var define = undefined;\n"
            f"{api_src}\n"
            "  globalThis.__ELKAPI__ = module.exports.default || module.exports;\n"
            "})();"
        )
        kind = rt.eval(
            "typeof globalThis.__ELKAPI__ + ':' + "
            "typeof (globalThis.__ElkWorkerMod__ && globalThis.__ElkWorkerMod__.Worker)"
        )
        if kind != "function:function":
            raise RuntimeError(f"elkjs failed to initialize in jsrun (got {kind!r})")
        _runtime = rt
        _elk_ready = True
        return _runtime


def _run_async(coro):
    """Run ``coro`` whether or not a loop is already running."""
    try:
        asyncio.get_running_loop()
    except RuntimeError:
        return asyncio.run(coro)

    out: dict[str, object] = {}

    def _target() -> None:
        out["value"] = asyncio.run(coro)

    t = threading.Thread(target=_target, name="elk-jsrun")
    t.start()
    t.join()
    if "value" not in out:
        raise RuntimeError("elk jsrun worker thread failed")
    return out["value"]


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


def _row_positions(
    layouts: Sequence[MoleculeLayout], *, gap: float = _GAP, center_y: bool = False
) -> list[tuple[float, float]]:
    sizes = [normalize_coords(L)[1:] for L in layouts]
    max_h = max((h for _w, h in sizes), default=0.0)
    x = 0.0
    positions: list[tuple[float, float]] = []
    for w, h in sizes:
        y = (max_h - h) * 0.5 if center_y else 0.0
        positions.append((x, y))
        x += w + gap
    return positions


def _reaction_defaults(spec: PictSpec) -> dict[str, str]:
    """ELK defaults for network / reaction diagrams (caller options win)."""
    base = {
        "elk.algorithm": "layered",
        "elk.direction": "RIGHT",
        "elk.edgeRouting": "ORTHOGONAL",
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
    return base


def elk_graph(layouts: Sequence[MoleculeLayout], spec: PictSpec) -> dict:
    nodes = []
    for i, L in enumerate(layouts):
        w, h = normalize_coords(L)[1:]
        nodes.append({"id": L.id or f"m{i}", "width": w, "height": h})
    edges = [
        {
            "id": f"e{i}",
            "sources": [e.source],
            "targets": [e.target],
            # Labels reserved for our SVG overlay; ELK still spaces for routes.
        }
        for i, e in enumerate(spec.diagram.edges)
    ]
    return {
        "id": "root",
        "layoutOptions": {
            **_reaction_defaults(spec),
            **{str(k): str(v) for k, v in spec.diagram.elk_options.items()},
        },
        "children": nodes,
        "edges": edges,
    }


def elk_graph_json(layouts: Sequence[MoleculeLayout], spec: PictSpec) -> str:
    return json.dumps(elk_graph(layouts, spec), indent=2)


async def _elk_layout_async(graph: dict) -> dict:
    rt = _ensure_elk_runtime()
    graph_json = json.dumps(graph)
    rt.eval(f"globalThis.__elkGraphJson__ = {json.dumps(graph_json)};")
    result_json = await rt.eval_async(
        """
        (async () => {
          const ELK = globalThis.__ELKAPI__;
          const Worker = globalThis.__ElkWorkerMod__.Worker;
          const elk = new ELK({ workerFactory: () => new Worker() });
          const laid = await elk.layout(JSON.parse(globalThis.__elkGraphJson__));
          return JSON.stringify(laid);
        })()
        """
    )
    if not isinstance(result_json, str):
        raise TypeError(f"expected JSON string from elkjs, got {type(result_json)!r}")
    return json.loads(result_json)


def _section_points(section: dict) -> list[tuple[float, float]]:
    """Flatten one ELK edge section into a polyline (start → bends → end)."""
    start = section.get("startPoint") or {}
    end = section.get("endPoint") or {}
    pts: list[tuple[float, float]] = [
        (float(start.get("x", 0.0)), float(start.get("y", 0.0)))
    ]
    for bp in section.get("bendPoints") or []:
        pts.append((float(bp.get("x", 0.0)), float(bp.get("y", 0.0))))
    pts.append((float(end.get("x", 0.0)), float(end.get("y", 0.0))))
    # Drop consecutive duplicates (ELK sometimes emits zero-length stubs).
    cleaned: list[tuple[float, float]] = []
    for p in pts:
        if not cleaned or abs(p[0] - cleaned[-1][0]) > 1e-6 or abs(p[1] - cleaned[-1][1]) > 1e-6:
            cleaned.append(p)
    return cleaned


def _edge_path_from_elk(edge: dict) -> list[tuple[float, float]] | None:
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


def _elkjs_placement(
    layouts: Sequence[MoleculeLayout], spec: PictSpec
) -> DiagramPlacement | None:
    """Run elkjs inside jsrun when available; include edge bend routes."""
    graph = elk_graph(layouts, spec)
    try:
        laid = _run_async(_elk_layout_async(graph))
    except Exception as exc:
        warnings.warn(
            f"elkjs (jsrun) layout failed ({exc}); falling back.",
            PictBackendWarning,
            stacklevel=3,
        )
        return None

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


def layout_diagram_ex(
    layouts: Sequence[MoleculeLayout], spec: PictSpec
) -> DiagramPlacement:
    """Place molecule viewports; for network/reaction also return ELK edge routes."""
    if len(layouts) <= 1:
        return DiagramPlacement(positions=[(0.0, 0.0)], edge_paths=[])

    kind = spec.diagram.kind
    if kind in {DiagramKind.network, DiagramKind.reaction}:
        elk = _elkjs_placement(layouts, spec)
        if elk is not None:
            return elk
        warnings.warn(
            "ELK (jsrun) unavailable; using row layout for network/reaction diagrams.",
            PictBackendWarning,
            stacklevel=3,
        )
        if kind == DiagramKind.reaction:
            pos = _row_positions(layouts, gap=_REACTION_GAP, center_y=True)
        else:
            pos = _row_positions(layouts)
        return DiagramPlacement(positions=pos, edge_paths=[])

    if kind == DiagramKind.grid:
        cols = spec.diagram.columns or max(1, int(len(layouts) ** 0.5 + 0.5))
        return DiagramPlacement(positions=_grid_positions(layouts, cols), edge_paths=[])

    return DiagramPlacement(positions=_row_positions(layouts), edge_paths=[])


def layout_diagram(
    layouts: Sequence[MoleculeLayout], spec: PictSpec
) -> list[tuple[float, float]]:
    """Return top-left positions for each molecule viewport."""
    return layout_diagram_ex(layouts, spec).positions
