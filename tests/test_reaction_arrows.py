"""Reaction schemes and diagram edge arrow styling."""

from __future__ import annotations

import warnings

import pytest

from xenosite.pict import Pict, render
from xenosite.pict.contracts.scene import PathPrim, Viewport
from xenosite.pict.contracts.spec import EdgeArrow, EdgeSpec, PictSpec
from xenosite.pict.diagram.elk import elk_graph, layout_diagram
from xenosite.pict.draw.arrows import diagram_overlays, edge_anchors, edge_primitives
from xenosite.pict.draw.scene_builder import build_scene
from xenosite.pict.warnings import PictBackendWarning


def test_edge_spec_arrow_fields():
    e = EdgeSpec.model_validate(
        {
            "source": "A",
            "target": "B",
            "label": "ADH",
            "role": "enzyme",
            "arrow": "equilibrium",
            "color": "#06c",
            "stroke_width": 2.0,
            "dashed": True,
        }
    )
    assert e.arrow == EdgeArrow.equilibrium
    assert e.dashed is True
    assert e.role == "enzyme"


def test_edge_anchors_on_viewport_boundary():
    a = Viewport(id="A", x=0, y=0, width=100, height=80)
    b = Viewport(id="B", x=200, y=0, width=100, height=80)
    (x1, y1), (x2, y2) = edge_anchors(a, b, pad=0)
    assert x1 == pytest.approx(100.0)
    assert x2 == pytest.approx(200.0)
    assert y1 == pytest.approx(40.0)
    assert y2 == pytest.approx(40.0)


def test_forward_arrow_has_shaft_and_head():
    a = Viewport(id="A", x=0, y=0, width=40, height=40)
    b = Viewport(id="B", x=120, y=0, width=40, height=40)
    prims = edge_primitives(EdgeSpec(source="A", target="B", label="ox"), a, b)
    paths = [p for p in prims if isinstance(p, PathPrim)]
    assert len(paths) >= 2
    assert any(p.fill and p.fill != "none" for p in paths)  # filled head
    assert any(getattr(p, "cls", None) and "label" in (p.cls or "") for p in prims)


def test_dashed_and_open_and_equilibrium():
    a = Viewport(id="A", x=0, y=0, width=40, height=40)
    b = Viewport(id="B", x=140, y=20, width=40, height=40)
    dashed = edge_primitives(
        EdgeSpec(source="A", target="B", arrow=EdgeArrow.forward, dashed=True), a, b
    )
    assert any(
        isinstance(p, PathPrim) and p.stroke_dasharray for p in dashed
    )
    open_p = edge_primitives(
        EdgeSpec(source="A", target="B", arrow=EdgeArrow.open), a, b
    )
    assert any(isinstance(p, PathPrim) and (p.fill in (None, "none")) for p in open_p)
    eq = edge_primitives(
        EdgeSpec(source="A", target="B", arrow=EdgeArrow.equilibrium), a, b
    )
    assert len([p for p in eq if isinstance(p, PathPrim)]) >= 4


def test_reaction_scheme_svg_draws_overlays():
    doc = PictSpec.model_validate(
        {
            "molecules": [
                {"id": "etoh", "smiles": "CCO"},
                {"id": "ach", "smiles": "CC=O"},
                {"id": "acetate", "smiles": "CC(=O)O"},
            ],
            "diagram": {
                "kind": "reaction",
                "edges": [
                    {
                        "source": "etoh",
                        "target": "ach",
                        "label": "ADH",
                        "arrow": "forward",
                    },
                    {
                        "source": "ach",
                        "target": "acetate",
                        "label": "ALDH",
                        "arrow": "forward",
                        "color": "#064",
                    },
                ],
            },
        }
    )
    pict = Pict(backend="native")
    layouts = pict.layout(doc).molecules
    with warnings.catch_warnings(record=True):
        warnings.simplefilter("always")
        positions = layout_diagram(layouts, doc)
    scene = build_scene(layouts, doc.molecules, doc, positions=positions)
    assert scene.overlays
    assert any(isinstance(p, PathPrim) and "edge" in (p.cls or "") for p in scene.overlays)
    svg = render(doc, backend="native")
    assert 'class="pict-overlays"' in svg or "pict-overlays" in svg
    assert "ADH" in svg
    assert "ALDH" in svg
    assert "edge-0" in svg


def test_network_dashed_edge_in_svg():
    doc = PictSpec.model_validate(
        {
            "molecules": [
                {"id": "A", "smiles": "CCO"},
                {"id": "B", "smiles": "C"},
            ],
            "diagram": {
                "kind": "network",
                "edges": [
                    {
                        "source": "A",
                        "target": "B",
                        "arrow": "line",
                        "dashed": True,
                        "label": "weak",
                    }
                ],
            },
        }
    )
    svg = render(doc, backend="native")
    assert "stroke-dasharray" in svg
    assert "weak" in svg


def test_reaction_elk_defaults_wider_spacing():
    doc = PictSpec.model_validate(
        {
            "molecules": [
                {"id": "A", "smiles": "C"},
                {"id": "B", "smiles": "O"},
            ],
            "diagram": {
                "kind": "reaction",
                "edges": [{"source": "A", "target": "B"}],
            },
        }
    )
    layouts = Pict(backend="native").layout(doc).molecules
    graph = elk_graph(layouts, doc)
    opts = graph["layoutOptions"]
    assert opts["elk.direction"] == "RIGHT"
    assert float(opts["elk.spacing.nodeNode"]) >= 64


def test_diagram_overlays_skips_missing_ids():
    a = Viewport(id="A", x=0, y=0, width=40, height=40)
    prims = diagram_overlays(
        [EdgeSpec(source="A", target="missing")],
        [a],
    )
    assert prims == []


def test_reaction_row_fallback_centers(monkeypatch):
    """When ELK fails, reaction fallback uses centered row with wider gap."""
    import xenosite.pict.diagram.elk as elk_mod

    monkeypatch.setattr(elk_mod, "_elkjs_positions", lambda *a, **k: None)
    doc = PictSpec.model_validate(
        {
            "molecules": [
                {"id": "A", "smiles": "CCO"},
                {"id": "B", "smiles": "C"},
                {"id": "C", "smiles": "CC(=O)O"},
            ],
            "diagram": {
                "kind": "reaction",
                "edges": [
                    {"source": "A", "target": "B"},
                    {"source": "B", "target": "C"},
                ],
            },
        }
    )
    layouts = Pict(backend="native").layout(doc).molecules
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        positions = layout_diagram(layouts, doc)
    assert any(issubclass(w.category, PictBackendWarning) for w in caught)
    assert len(positions) == 3
    # Vertically centered: smaller molecule should have y > 0 when heights differ.
    ys = [p[1] for p in positions]
    assert min(ys) >= 0.0
    xs = [p[0] for p in positions]
    assert xs[1] - xs[0] > 40  # reaction gap leaves room for arrows
