"""SVG root width/height drive intrinsic layout (no stretch-to-column)."""

from __future__ import annotations

import re

from xpict import Pict
from xpict.draw.svg import scene_to_html


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


def test_scene_to_html_preserves_intrinsic_css():
    html = Pict(backend="native", format="html").render(
        {"molecules": [{"smiles": "CCO"}]}
    )
    assert "width: auto" in html
    assert "height: auto" in html
    assert "inline-block" in html
    assert "flex: 0 0 auto" in html
    # Do not force every mol SVG to fill the page width.
    assert "max-width: 100%; height: auto; display: block" not in html
    attrs = _root_attrs(html)
    assert attrs["width"].endswith("px")
    assert attrs["height"].endswith("px")
    # scene_to_html imports still used by format=html path
    assert scene_to_html  # noqa: B018 — imported for API stability
