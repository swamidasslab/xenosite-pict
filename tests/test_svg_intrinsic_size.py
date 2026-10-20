"""SVG root width/height + data-URI <img> for intrinsic layout (xenosite pattern)."""

from __future__ import annotations


import re
from urllib.parse import unquote

from xpict import Pict
from xpict.rendered import svg_to_data_uri, svg_to_img_tag

# Drawing units ≈ CSS px for data-URI <img> (Rust SCALE is 30).
SCALE = 30.0

_ROOT = re.compile(r"<svg\b([^>]*)>", re.DOTALL)
_ATTR = re.compile(r'([\w:-]+)="([^"]*)"')


def _root_attrs(markup: str) -> dict[str, str]:
    m = _ROOT.search(markup)
    assert m is not None
    return dict(_ATTR.findall(m.group(1)))


def test_scale_default_is_thirty():
    assert SCALE == 30.0


def test_render_svg_sets_width_height_matching_viewbox():
    """Like xenosite acetone: width/height == viewBox size (drawing units)."""
    svg = Pict().render({"molecules": [{"smiles": "CCO"}]})
    attrs = _root_attrs(svg)
    w = float(attrs["width"].removesuffix("px"))
    h = float(attrs["height"].removesuffix("px"))
    assert w > 0 and h > 0
    vb = attrs["viewBox"].split()
    assert len(vb) == 4
    assert float(vb[2]) == w
    assert float(vb[3]) == h
    # Mean bond draws near SCALE, so a small mol is tens of CSS px (visible).
    assert w >= SCALE


def test_separate_mol_svgs_keep_distinct_intrinsic_sizes():
    """Different molecules keep their own intrinsic SVG sizes."""
    pict = Pict()
    etoh = pict.render({"molecules": [{"smiles": "CCO"}]})
    phenol = pict.render({"molecules": [{"smiles": "c1ccccc1O"}]})
    ew = float(_root_attrs(etoh)["width"].removesuffix("px"))
    pw = float(_root_attrs(phenol)["width"].removesuffix("px"))
    assert ew != pw
    assert ew >= SCALE and pw >= SCALE


def test_scene_to_html_embeds_data_uri_img_xenosite_style():
    html = Pict(format="html").render({"molecules": [{"smiles": "CCO"}]})
    assert 'src="data:image/svg+xml;utf8,' in html
    assert '<img class="xpict"' in html
    assert 'class="xpict-mol"' in html
    assert "max-width: 100%" in html
    assert "max-height: 100%" in html
    assert "height: auto" in html
    body = html.split("<main", 1)[1]
    assert "<svg" not in body


def test_svg_to_data_uri_roundtrips_markup():
    svg = Pict().render({"molecules": [{"smiles": "CCO"}]})
    uri = svg_to_data_uri(svg)
    assert uri.startswith("data:image/svg+xml;utf8,")
    payload = unquote(uri.split(",", 1)[1])
    assert payload.lstrip().startswith("<svg")
    tag = svg_to_img_tag(svg, alt="ethanol")
    assert 'alt="ethanol"' in tag
    assert 'width="' in tag and 'height="' in tag
