"""Global halo knockouts from ink shapes."""

from __future__ import annotations

import re

from xpict import render
from xpict.draw.glyphs import compile_text_shapes
from xpict.draw.halo import halo_from_shapes, halo_path_d
from xpict.draw.metrics import HALO_GAP_PX


def test_halo_preserves_o_counter():
    ink = compile_text_shapes("O", 20.0, 20.0, font_size=14.0)
    assert ink is not None
    assert ink.has_holes
    halo = halo_from_shapes(ink, HALO_GAP_PX)
    assert halo is not None and not halo.is_empty
    cx, cy = ink.centroid
    # Center of O stays open…
    assert not halo.contains(cx, cy)
    # …but the offset inner rim (just inside the ink hole) is haloed.
    for step in (i * 0.25 for i in range(1, 40)):
        if ink.contains(cx + step, cy):
            assert halo.contains(cx + step - HALO_GAP_PX * 0.5, cy)
            break
    else:
        raise AssertionError("expected ink ring around O centroid")


def test_halo_global_off_emits_no_halo():
    svg = render(
        {"molecules": [{"smiles": "CCO"}], "halo": False},
        backend="native",
    )
    # Empty halo layer group may exist; no knockout paths.
    assert not re.search(r'<path[^>]*class="[^"]*\bhalo\b', svg)


def test_halo_layer_is_first_in_viewport():
    from xpict import Pict
    from xpict.contracts.nodes import PictSpec
    from xpict.draw.scene_builder import build_scene

    pict = Pict(backend="native")
    layouts = pict.layout({"molecules": [{"smiles": "CCO"}]}).molecules
    spec = PictSpec.model_validate({"molecules": [{"smiles": "CCO"}], "halo": True})
    scene = build_scene(layouts, spec.molecules, spec.to_legacy())
    names = [layer.name for layer in scene.viewports[0].layers]
    # xenopict: shading → mol_halo → lines (halo cuts channels through shade).
    assert names[0] == "shading"
    assert names[1] == "halo"
    assert names.index("halo") < names.index("bonds")
    assert names.index("halo") < names.index("labels")


def test_legacy_molecule_halo_lifts_to_document():
    from xpict.contracts.nodes import PictSpec

    spec = PictSpec.model_validate(
        {"molecules": [{"smiles": "CCO", "halo": False}]}
    )
    assert spec.halo is False


def test_shading_does_not_opt_into_document_halo():
    """Shade disks paint under the knockout but never join it."""
    from xpict import Pict
    from xpict.contracts.nodes import PictSpec
    from xpict.draw.drawable import paint_molecule

    bare = PictSpec.model_validate({"molecules": [{"smiles": "CCO"}], "halo": True})
    shaded = PictSpec.model_validate(
        {
            "molecules": [
                {
                    "smiles": "CCO",
                    "shade": {"atoms": [0.2, 0.5, 0.9], "vmin": 0.0, "vmax": 1.0},
                }
            ],
            "halo": True,
        }
    )
    pict = Pict(backend="native")
    bare_layout = pict.layout(bare).molecules[0]
    shaded_layout = pict.layout(shaded).molecules[0]
    _, bare_halo = paint_molecule(bare_layout, bare.molecules[0], halo=True)
    vp, shaded_halo = paint_molecule(shaded_layout, shaded.molecules[0], halo=True)
    assert any(
        getattr(p, "cls", None) == "shade"
        for layer in vp.layers
        for p in layer.primitives
    )
    assert len(shaded_halo.jobs) == len(bare_halo.jobs)
    assert shaded_halo.to_prim().d == bare_halo.to_prim().d
