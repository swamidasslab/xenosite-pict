"""Global halo knockouts from ink shapes."""

from __future__ import annotations

import re

from shapely.geometry import Point

from xenosite.pict import render
from xenosite.pict.draw.glyphs import compile_text_shapes
from xenosite.pict.draw.halo import halo_from_shapes, halo_path_d
from xenosite.pict.draw.metrics import LABEL_GAP_PX


def test_halo_preserves_o_counter():
    ink = compile_text_shapes("O", 20.0, 20.0, font_size=14.0)
    assert ink is not None
    # Glyph O has an interior hole.
    polys = list(getattr(ink, "geoms", [ink]))
    assert any(getattr(p, "interiors", ()) for p in polys)
    halo = halo_from_shapes(ink, LABEL_GAP_PX)
    assert halo is not None and not halo.is_empty
    # Center of O should not be covered by the white halo.
    cx = ink.centroid.x
    cy = ink.centroid.y
    assert not halo.contains(Point(cx, cy))


def test_halo_global_off_emits_no_halo():
    svg = render(
        {"molecules": [{"smiles": "CCO"}], "halo": False},
        backend="native",
    )
    # Empty halo layer group may exist; no knockout paths.
    assert not re.search(r'<path[^>]*class="[^"]*\bhalo\b', svg)


def test_halo_layer_is_first_in_viewport():
    from xenosite.pict import Pict
    from xenosite.pict.contracts.spec import PictSpec
    from xenosite.pict.draw.scene_builder import build_scene

    pict = Pict(backend="native")
    layouts = pict.layout({"molecules": [{"smiles": "CCO"}]}).molecules
    spec = PictSpec.model_validate({"molecules": [{"smiles": "CCO"}], "halo": True})
    scene = build_scene(layouts, spec.molecules, spec)
    names = [layer.name for layer in scene.viewports[0].layers]
    assert names[0] == "halo"
    assert names.index("halo") < names.index("bonds")
    assert names.index("halo") < names.index("labels")


def test_legacy_molecule_halo_lifts_to_document():
    from xenosite.pict.contracts.spec import PictSpec

    spec = PictSpec.model_validate(
        {"molecules": [{"smiles": "CCO", "halo": False}]}
    )
    assert spec.halo is False
