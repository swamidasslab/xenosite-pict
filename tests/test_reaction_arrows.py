"""Reaction schemes and diagram edge arrow styling."""

from __future__ import annotations

from helpers import layout_backend

import warnings

import pytest

from xpict import Pict, render
from xpict.future.nodes import PictSpec
from xpict.contracts.scene import PathPrim, Viewport
from xpict.future.spec import EdgeArrow, EdgeSpec
from xpict.diagram.elk import elk_graph, layout_diagram
from xpict.draw.arrows import (
    diagram_overlays,
    edge_anchors,
    edge_primitives,
    filleted_path_d,
    resolve_route,
    simplify_route,
)
from xpict.draw.scene_builder import build_scene
from xpict.warnings import PictBackendWarning


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
    heads = [p for p in paths if p.fill and p.fill != "none"]
    assert heads  # filled head
    assert all(p.stroke in (None, "none") or p.stroke_width == 0 for p in heads)
    assert any(getattr(p, "cls", None) and "label" in (p.cls or "") for p in prims)


def test_filled_path_ink_buffers_stroke_growth():
    """When a fill still has a stroke, halo ink grows by half that stroke."""
    from xpict.draw.halo import halo_from_shapes
    from xpict.draw.metrics import HALO_GAP_PX
    from xpict.draw.paths import filled_arrow_head_d, ink_from_path_prim, path_coords

    tip = (100.0, 50.0)
    ux, uy = 1.0, 0.0
    # Stroked fill (e.g. wedge): ink must clear the painted tip.
    stroked = PathPrim(
        d=filled_arrow_head_d(tip[0], tip[1], ux, uy, size=9.0),
        stroke="#222",
        fill="#222",
        stroke_width=1.0,
        cls="bond bond-wedge-up",
    )
    ink = ink_from_path_prim(stroked)
    assert ink is not None
    assert ink.contains(tip[0] + ux * 0.25, tip[1] + uy * 0.25)
    assert ink.bounds[2] > tip[0]

    grown = halo_from_shapes(ink, HALO_GAP_PX)
    assert grown is not None
    assert grown.contains(tip[0] + ux * (0.5 + 0.5 * HALO_GAP_PX), tip[1])
    for x, y in path_coords(stroked.d)[:3]:
        assert grown.contains(x, y)


def test_diagram_arrows_do_not_opt_into_document_halo():
    """Edge shafts / heads / labels are drawn but skip the shared halo."""
    from xpict.draw.drawable import paint_molecule
    from xpict.draw.drawn import Halo

    doc = PictSpec.model_validate(
        {
            "molecules": [
                {"id": "A", "smiles": "CCO"},
                {"id": "B", "smiles": "CC=O"},
            ],
            "diagram": {
                "kind": "reaction",
                "edges": [{"source": "A", "target": "B", "label": "ADH", "arrow": "forward"}],
            },
            "halo": True,
        }
    )
    pict = Pict(backend=layout_backend())
    layouts = pict.layout(doc)
    scene = build_scene(layouts, doc.molecules, doc)
    assert any(isinstance(p, PathPrim) and "head" in (p.cls or "") for p in scene.overlays)
    assert any(getattr(p, "cls", None) and "label" in (p.cls or "") for p in scene.overlays)
    # Halo matches molecule opt-ins only (no overlay ink).
    expected = Halo()
    for layout, ms, vp in zip(layouts, doc.molecules, scene.viewports, strict=True):
        _, h = paint_molecule(layout, ms, halo=True)
        h.shift(vp.x, vp.y)
        expected.extend(h.jobs)
    assert scene.halo and expected.to_prim() is not None
    assert scene.halo[0].d == expected.to_prim().d


def test_molecule_caption_does_not_opt_into_document_halo():
    """Molecule titles draw without joining the shared halo."""
    from xpict.contracts.scene import TextPrim
    from xpict.draw.drawable import paint_molecule

    bare = PictSpec.model_validate({"molecules": [{"smiles": "CCO"}], "halo": True})
    titled = PictSpec.model_validate(
        {
            "molecules": [{"smiles": "CCO", "label": {"text": "ethanol", "pos": "bottom"}}],
            "halo": True,
        }
    )
    pict = Pict(backend=layout_backend())
    bare_layout = pict.layout(bare)[0]
    titled_layout = pict.layout(titled)[0]
    _, bare_halo = paint_molecule(bare_layout, bare.molecules[0], halo=True)
    vp, titled_halo = paint_molecule(titled_layout, titled.molecules[0], halo=True)
    assert any(
        isinstance(p, TextPrim) and p.cls == "mol-label"
        for layer in vp.layers
        for p in layer.primitives
    )
    # Same backbone + element-symbol ink; caption must not add jobs.
    assert len(titled_halo.jobs) == len(bare_halo.jobs)


def test_dashed_and_open_and_equilibrium():
    a = Viewport(id="A", x=0, y=0, width=40, height=40)
    b = Viewport(id="B", x=140, y=20, width=40, height=40)
    dashed = edge_primitives(
        EdgeSpec(source="A", target="B", arrow=EdgeArrow.forward, dashed=True), a, b
    )
    assert any(isinstance(p, PathPrim) and p.stroke_dasharray for p in dashed)
    open_p = edge_primitives(EdgeSpec(source="A", target="B", arrow=EdgeArrow.open), a, b)
    assert any(isinstance(p, PathPrim) and (p.fill in (None, "none")) for p in open_p)
    eq = edge_primitives(EdgeSpec(source="A", target="B", arrow=EdgeArrow.equilibrium), a, b)
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
    pict = Pict(backend=layout_backend())
    layouts = pict.layout(doc)
    with warnings.catch_warnings(record=True):
        warnings.simplefilter("always")
        positions = layout_diagram(layouts, doc)
    scene = build_scene(layouts, doc.molecules, doc, positions=positions)
    assert scene.overlays
    assert any(isinstance(p, PathPrim) and "edge" in (p.cls or "") for p in scene.overlays)
    svg = render(doc, backend=layout_backend())
    assert 'class="xpict-overlays"' in svg or "xpict-overlays" in svg
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
    svg = render(doc, backend=layout_backend())
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
                "edges": [{"source": "A", "target": "B", "label": "ADH", "label_pos": "below"}],
            },
        }
    )
    layouts = Pict(backend=layout_backend()).layout(doc)
    graph = elk_graph(layouts, doc)
    opts = graph["layoutOptions"]
    assert opts["elk.direction"] == "RIGHT"
    assert opts["elk.edgeRouting"] == "POLYLINE"
    assert float(opts["elk.spacing.nodeNode"]) == 20
    assert float(opts["elk.layered.spacing.nodeNodeBetweenLayers"]) == 20
    # Measured text boxes go to ELK with placement side.
    labs = graph["edges"][0]["labels"]
    assert len(labs) == 1
    assert labs[0]["text"] == "ADH"
    assert labs[0]["width"] > 8
    assert labs[0]["height"] > 6
    assert (
        labs[0]["layoutOptions"]["elk.layered.edgeLabels.sideSelection"]
        == "ALWAYS_DOWN"
    )


def test_reaction_packing_spacing_knobs():
    """node_spacing / layer_spacing tighten or loosen ELK packing."""
    tight = PictSpec.model_validate(
        {
            "molecules": [
                {"id": "A", "smiles": "CCO"},
                {"id": "B", "smiles": "CC=O"},
            ],
            "diagram": {
                "kind": "reaction",
                "node_spacing": 24,
                "layer_spacing": 36,
                "edges": [{"source": "A", "target": "B"}],
            },
        }
    )
    loose = PictSpec.model_validate(
        {
            "molecules": [
                {"id": "A", "smiles": "CCO"},
                {"id": "B", "smiles": "CC=O"},
            ],
            "diagram": {
                "kind": "reaction",
                "node_spacing": 80,
                "layer_spacing": 120,
                "edges": [{"source": "A", "target": "B"}],
            },
        }
    )
    pict = Pict(backend=layout_backend())
    tg = elk_graph(pict.layout(tight), tight)["layoutOptions"]
    lg = elk_graph(pict.layout(loose), loose)["layoutOptions"]
    assert float(tg["elk.spacing.nodeNode"]) == 24
    assert float(tg["elk.layered.spacing.nodeNodeBetweenLayers"]) == 36
    assert float(lg["elk.spacing.nodeNode"]) == 80
    assert float(lg["elk.layered.spacing.nodeNodeBetweenLayers"]) == 120
    # Tighter packing should place B closer to A along x.
    tp = layout_diagram(pict.layout(tight), tight)
    lp = layout_diagram(pict.layout(loose), loose)
    assert lp[1][0] - lp[0][0] > tp[1][0] - tp[0][0]


def test_diagram_overlays_skips_missing_ids():
    a = Viewport(id="A", x=0, y=0, width=40, height=40)
    prims = diagram_overlays(
        [EdgeSpec(source="A", target="missing")],
        [a],
    )
    assert prims == []


def test_reaction_row_fallback_centers(monkeypatch):
    """When ELK fails, reaction fallback uses centered row with wider gap."""
    import xpict.diagram.elk as elk_mod

    monkeypatch.setattr(elk_mod, "_elkrs_placement", lambda *a, **k: None)
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
    layouts = Pict(backend=layout_backend()).layout(doc)
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


def test_branched_reaction_uses_elk_routes():
    """Forked pathway: ELK places nodes in 2D and returns bend polylines."""
    from xpict.diagram.elk import layout_diagram_ex

    doc = PictSpec.model_validate(
        {
            "molecules": [
                {"id": "A", "smiles": "CCO"},
                {"id": "B", "smiles": "CC=O"},
                {"id": "C", "smiles": "CC(=O)O"},
                {"id": "D", "smiles": "c1ccccc1"},
                {"id": "E", "smiles": "c1ccccc1O"},
            ],
            "diagram": {
                "kind": "reaction",
                # Orthogonal routes produce bend points; default polyline may be straight.
                "elk_options": {"elk.edgeRouting": "ORTHOGONAL"},
                "edges": [
                    {"source": "A", "target": "B", "label": "ADH"},
                    {"source": "B", "target": "C", "label": "ALDH"},
                    {"source": "A", "target": "D", "label": "arom"},
                    {"source": "D", "target": "E", "label": "CYP"},
                    {
                        "source": "B",
                        "target": "E",
                        "label": "side",
                        "dashed": True,
                        "color": "#a40",
                    },
                ],
            },
        }
    )
    layouts = Pict(backend=layout_backend()).layout(doc)
    place = layout_diagram_ex(layouts, doc)
    assert len(place.positions) == 5
    # Not a single horizontal line — branches occupy distinct Y bands.
    ys = [round(y, 0) for _x, y in place.positions]
    assert len(set(ys)) >= 2
    assert place.edge_paths
    assert len(place.edge_paths) == 5
    # At least one routed edge has an orthogonal bend (3+ points).
    assert any(p is not None and len(p) >= 3 for p in place.edge_paths)

    scene = build_scene(
        layouts,
        doc.molecules,
        doc,
        positions=place.positions,
        edge_paths=place.edge_paths,
        diagram_width=place.width,
        diagram_height=place.height,
    )
    # Multi-segment path data (orthogonal route).
    assert any(isinstance(p, PathPrim) and p.d.count("L") >= 2 for p in scene.overlays)
    svg = render(doc, backend=layout_backend())
    assert "pict-overlays" in svg
    assert "ADH" in svg and "CYP" in svg and "side" in svg
    assert "stroke-dasharray" in svg


def test_simplify_route_snaps_micro_kink_to_straight():
    """Orthogonal / polyline jogs under the kink threshold become a straight shaft."""
    # Classic 5px jog — must collapse to two endpoints.
    jog = [(0.0, 0.0), (100.0, 0.0), (100.0, 5.0), (200.0, 5.0)]
    flat = simplify_route(jog, kink_px=10.0)
    assert len(flat) == 2
    assert flat[0] == (0.0, 0.0)
    assert flat[-1] == (200.0, 5.0)

    # Real L-bend stays (arms well above threshold).
    elbow = [(0.0, 0.0), (80.0, 0.0), (80.0, 60.0), (140.0, 60.0)]
    kept = simplify_route(elbow, kink_px=10.0)
    assert len(kept) == 4


def test_resolve_route_applies_simplify():
    a = Viewport(id="A", x=0, y=0, width=40, height=40)
    b = Viewport(id="B", x=200, y=0, width=40, height=40)
    route = [(40.0, 20.0), (120.0, 20.0), (120.0, 28.0), (200.0, 28.0)]
    pts = resolve_route(a, b, route)
    assert len(pts) == 2


def test_filleted_path_has_quadratic_turns():
    pts = [(0.0, 0.0), (80.0, 0.0), (80.0, 60.0), (140.0, 60.0)]
    d = filleted_path_d(pts, radius=18.0)
    assert "Q" in d
    # Straight two-point path stays linear.
    assert "Q" not in filleted_path_d([(0.0, 0.0), (10.0, 0.0)])


def test_polyline_route_drawn_with_bends():
    a = Viewport(id="A", x=0, y=0, width=40, height=40)
    b = Viewport(id="B", x=100, y=80, width=40, height=40)
    route = [(40.0, 20.0), (70.0, 20.0), (70.0, 100.0), (100.0, 100.0)]
    prims = edge_primitives(
        EdgeSpec(source="A", target="B", label="bend"),
        a,
        b,
        route=route,
    )
    paths = [p for p in prims if isinstance(p, PathPrim)]
    shaft = next(p for p in paths if "head" not in (p.cls or ""))
    # Real bends keep corners; fillets paint them as quadratic arcs.
    assert "Q" in shaft.d
    assert shaft.d.count("L") >= 1
    assert "bend" in " ".join(getattr(p, "text", "") or "" for p in prims)


def test_micro_kink_route_draws_straight_shaft():
    """Tiny jog snaps to a straight shaft (no fillet / no kink)."""
    a = Viewport(id="A", x=0, y=0, width=40, height=40)
    b = Viewport(id="B", x=200, y=0, width=40, height=40)
    route = [(40.0, 20.0), (120.0, 20.0), (120.0, 26.0), (200.0, 26.0)]
    prims = edge_primitives(EdgeSpec(source="A", target="B"), a, b, route=route)
    paths = [p for p in prims if isinstance(p, PathPrim)]
    shaft = next(p for p in paths if "head" not in (p.cls or ""))
    assert "Q" not in shaft.d
    # Single line segment after simplify.
    assert shaft.d.count("L") == 1
