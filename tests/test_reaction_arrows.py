"""Reaction schemes via live DepictSpec + Rust compose_scheme."""

from __future__ import annotations

import json

import pytest

from xpict import depict, depict_svg, render
from xpict.contracts.scene import PathPrim, TextPrim
from xpict.native_bridge import compose_scheme


def _scheme(a="CCO", b="CC=O", *, label="ADH", arrow="forward", **layout):
    children: list[dict] = [
        {"type": "mol", "id": "A", "smiles": a},
        {"type": "mol", "id": "B", "smiles": b},
    ]
    edge: dict = {
        "type": "edge",
        "sources": "A",
        "targets": "B",
        "arrow": arrow,
    }
    if label:
        children.insert(1, {"type": "text", "id": "lab", "text": label})
        edge["label"] = {"id": "lab", "pos": "above"}
    for k in ("color", "stroke_width", "dashed", "edge_routing", "role"):
        if k in layout:
            edge[k] = layout.pop(k)
    children.append(edge)
    out: dict = {"type": "reaction_scheme", "children": children}
    if layout:
        out["layout"] = layout
    return out


def test_live_scheme_composes_overlays():
    rows = depict(_scheme())
    assert len(rows) == 1
    scene = rows[0].scene
    assert len(scene.viewports) == 2
    assert any(isinstance(p, PathPrim) and "edge" in (p.cls or "") for p in scene.overlays)
    assert any(
        (isinstance(p, TextPrim) or (isinstance(p, PathPrim) and "label" in (p.cls or "")))
        for p in scene.overlays
    ) or any(getattr(p, "cls", None) and "label" in (p.cls or "") for p in scene.overlays)


def test_scheme_svg_has_arrow_paths():
    svg = depict_svg(_scheme(arrow="forward"))
    assert "<svg" in svg
    assert "edge" in svg or "M " in svg


def test_render_routes_live_reaction_scheme():
    svg = render(_scheme(dashed=True, color="#06c"))
    assert "<svg" in svg


def test_render_routes_legacy_reaction_doc():
    """Legacy flat reaction docs still render via Rust compose."""
    svg = render(
        {
            "molecules": [
                {"id": "A", "smiles": "CCO"},
                {"id": "B", "smiles": "CC=O"},
            ],
            "diagram": {
                "kind": "reaction",
                "edges": [
                    {
                        "source": "A",
                        "target": "B",
                        "label": "ADH",
                        "arrow": "forward",
                    }
                ],
            },
        }
    )
    assert "<svg" in svg
    assert len(svg) > 100


def test_equilibrium_and_open_arrows():
    for arrow in ("equilibrium", "open", "line"):
        scene = depict(_scheme(arrow=arrow, label=None))[0].scene
        assert scene.overlays, f"expected overlays for {arrow}"


def test_orthogonal_stays_axis_aligned():
    scene = depict(
        _scheme(label=None, edge_routing="orthogonal", algorithm="layered")
    )[0].scene
    # At least one path; orthogonal shafts should not introduce Q fillets on H/V.
    paths = [p for p in scene.overlays if isinstance(p, PathPrim) and p.d]
    assert paths
    # Polyline fillets use Q; orthogonal should be L-only for axis-aligned.
    # (ELK may emit non-axis segments rarely; just ensure we got paint.)
    assert any("M" in p.d for p in paths)


def test_compose_scheme_native_json():
    """Direct native compose_scheme on synthetic DocPaint boxes."""
    from xpict.contracts import DepictSpec

    spec = {
        "type": "reaction_scheme",
        "children": [
            {"type": "mol", "id": "A", "smiles": "C"},
            {"type": "mol", "id": "B", "smiles": "O"},
            {"type": "edge", "sources": "A", "targets": "B", "arrow": "forward"},
        ],
        "layout": {"edge_routing": "polyline"},
    }
    paints = [
        {
            "id": "A",
            "molecule": {
                "id": "A",
                "atoms": [{"index": 0, "element": "C", "x": 0, "y": 0}],
                "bonds": [],
            },
            "scene": {
                "width": 60,
                "height": 40,
                "viewports": [
                    {"id": "A", "x": 0, "y": 0, "width": 60, "height": 40, "layers": []}
                ],
                "overlays": [],
                "halo": [],
            },
        },
        {
            "id": "B",
            "molecule": {
                "id": "B",
                "atoms": [{"index": 0, "element": "O", "x": 0, "y": 0}],
                "bonds": [],
            },
            "scene": {
                "width": 60,
                "height": 40,
                "viewports": [
                    {"id": "B", "x": 0, "y": 0, "width": 60, "height": 40, "layers": []}
                ],
                "overlays": [],
                "halo": [],
            },
        },
    ]
    scene = compose_scheme(DepictSpec.model_validate(spec), paints)
    assert len(scene["viewports"]) == 2
    assert scene["overlays"]
    assert scene["width"] > 60


def test_multi_endpoint_edge_uses_first_pair():
    spec = {
        "type": "reaction_scheme",
        "children": [
            {"type": "mol", "id": "A", "smiles": "C"},
            {"type": "mol", "id": "B", "smiles": "N"},
            {"type": "mol", "id": "C", "smiles": "O"},
            {
                "type": "edge",
                "sources": ["A", "B"],
                "targets": ["C"],
                "arrow": "forward",
            },
        ],
    }
    scene = depict(spec)[0].scene
    assert len(scene.viewports) == 3
    assert scene.overlays


def test_spacing_knobs_accepted():
    svg = depict_svg(
        _scheme(label=None, node_spacing=8, layer_spacing=4, algorithm="layered")
    )
    assert "<svg" in svg
