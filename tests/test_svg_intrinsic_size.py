"""SVG root width/height + data-URI <img> for intrinsic layout."""

from __future__ import annotations

import re
from urllib.parse import unquote

from xpict import Pict
from xpict.draw.svg import svg_to_data_uri, svg_to_img_tag


_ROOT = re.compile(r"<svg\b([^>]*)>", re.DOTALL)
_ATTR = re.compile(r'([\w:-]+)="([^"]*)"')


def _root_attrs(markup: str) -> dict[str, str]:
    m = _ROOT.search(markup)
    assert m is not None
    return dict(_ATTR.findall(m.group(1)))


def test_render_svg_sets_px_width_height_and_viewbox():
    svg = Pict(backend="native").render({"molecules": [{"smiles": "CCO"}]})
    attrs = _root_attrs(svg)
    assert attrs["width"].endswith("px")
    assert attrs["height"].endswith("px")
    w = float(attrs["width"].removesuffix("px"))
    h = float(attrs["height"].removesuffix("px"))
    assert w > 0 and h > 0
    vb = attrs["viewBox"].split()
    assert len(vb) == 4
    assert float(vb[2]) == w
    assert float(vb[3]) == h


def test_separate_mol_svgs_keep_distinct_intrinsic_sizes():
    """Ethanol is narrower than phenol; both keep their own px size."""
    pict = Pict(backend="native")
    etoh = pict.render({"molecules": [{"smiles": "CCO"}]})
    phenol = pict.render({"molecules": [{"smiles": "c1ccccc1O"}]})
    ew = float(_root_attrs(etoh)["width"].removesuffix("px"))
    pw = float(_root_attrs(phenol)["width"].removesuffix("px"))
    eh = float(_root_attrs(etoh)["height"].removesuffix("px"))
    ph = float(_root_attrs(phenol)["height"].removesuffix("px"))
    assert ew < pw
    assert eh > 0 and ph > 0


def test_scene_to_html_embeds_data_uri_img():
    html = Pict(backend="native", format="html").render(
        {"molecules": [{"smiles": "CCO"}]}
    )
    assert 'src="data:image/svg+xml;charset=utf-8,' in html
    assert "<img class=\"xpict\"" in html
    assert re.search(r'<img[^>]*\bwidth="[0-9.]+"', html)
    assert re.search(r'<img[^>]*\bheight="[0-9.]+"', html)
    # No inline <svg> in the page body — sizing comes from the image intrinsic.
    body = html.split("<main", 1)[1]
    assert "<svg" not in body
    assert "img.xpict" in html
    assert "width: auto" in html


def test_svg_to_data_uri_roundtrips_markup():
    svg = Pict(backend="native").render({"molecules": [{"smiles": "CCO"}]})
    uri = svg_to_data_uri(svg)
    assert uri.startswith("data:image/svg+xml;charset=utf-8,")
    payload = unquote(uri.split(",", 1)[1])
    assert payload.lstrip().startswith("<svg")
    assert 'width="' in payload
    tag = svg_to_img_tag(svg, alt="ethanol")
    assert 'alt="ethanol"' in tag
    assert 'width="' in tag and 'height="' in tag
