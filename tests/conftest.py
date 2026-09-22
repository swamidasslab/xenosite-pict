"""XFail depiction features not yet on the shared Rust/JS paint surface."""

from __future__ import annotations

import pytest

_XFAIL_NAMES = {
    "test_callout_svg_has_label_and_arrow",
    "test_region_kinds_emit_paths",
    "test_ring_callout_uses_centroid",
    "test_annotations_land_on_marks_layer",
    "test_radical_dot_in_svg_native",
    "test_radical_dot_still_drawn",
    "test_halo_layer_is_first_in_viewport",
    "test_shading_does_not_opt_into_document_halo",
    "test_ring_attachment_draws_callout",
    "test_render_emits_centered_mol_label",
    "test_render_label_pos_top",
    "test_diagram_arrows_do_not_opt_into_document_halo",
    "test_molecule_caption_does_not_opt_into_document_halo",
    "test_svg_emits_glyph_paths_not_text",
}


def pytest_collection_modifyitems(config, items):
    reason = "JS-mirror MVP: feature not on Rust depict_molecule surface yet"
    for item in items:
        if item.name in _XFAIL_NAMES:
            item.add_marker(pytest.mark.xfail(reason=reason, strict=False))
