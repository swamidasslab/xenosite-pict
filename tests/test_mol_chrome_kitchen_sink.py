"""Notebook kitchen-sink chrome: custom atom labels must not cover indices."""

from __future__ import annotations

import re

from xpict import mol
from xpict.contracts.depict import MolNode
from xpict.spec import DepictSpec

_PATH = re.compile(r"<path\b([^>]*)>", re.DOTALL)
_ATTR = re.compile(r'([\w:-]+)="([^"]*)"')
_NUM = re.compile(r"-?\d+(?:\.\d+)?")


def _attrs(tag: str) -> dict[str, str]:
    return dict(_ATTR.findall(tag))


def _bbox(d: str) -> tuple[float, float, float, float]:
    nums = [float(x) for x in _NUM.findall(d)]
    xs, ys = nums[0::2], nums[1::2]
    return min(xs), min(ys), max(xs), max(ys)


def _overlaps(
    a: tuple[float, float, float, float], b: tuple[float, float, float, float]
) -> bool:
    return a[0] < b[2] and a[2] > b[0] and a[1] < b[3] and a[3] > b[1]


def _kitchen_sink_svg() -> str:
    # Same chrome as notebooks/mol_chrome_progress.ipynb kitchen sink.
    base = mol("c1ccccc1O", name="phenol").to_node().model_dump(exclude_none=True)
    base.update(
        {
            "shade": {"atoms": [0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0]},
            "atom_number": "index",
            "atom_label": {"2": "ortho", "6": "exo"},
            "circle": {"atoms": [6], "stroke": {"color": "#06c", "width": 1.4}},
            "region": {
                "style": "box",
                "atoms": [0, 1, 2, 3, 4, 5],
                "fill": {"color": "#48a", "opacity": 0.12},
                "stroke": {"color": "#48a", "width": 1.0},
                "text": "ring",
            },
            "callout": {
                "atoms": [6],
                "text": "phenol OH",
                "arrow": True,
                "prefer": "above",
            },
        }
    )
    return DepictSpec(MolNode.model_validate(base))._repr_svg_()


def _label_boxes(svg: str) -> dict[tuple[str, str], tuple[float, float, float, float]]:
    out: dict[tuple[str, str], tuple[float, float, float, float]] = {}
    for tag in _PATH.findall(svg):
        attrs = _attrs(tag)
        cls = attrs.get("class", "")
        text = attrs.get("data-text")
        d = attrs.get("d")
        if not text or not d:
            continue
        kind = None
        if "atom-number" in cls:
            kind = "number"
        elif "atom-label" in cls:
            kind = "label"
        if kind is None:
            continue
        out[(kind, text)] = _bbox(d)
    return out


def test_kitchen_sink_ortho_does_not_cover_index_2():
    boxes = _label_boxes(_kitchen_sink_svg())
    two = boxes[("number", "2")]
    ortho = boxes[("label", "ortho")]
    assert not _overlaps(two, ortho), f"index 2 {two} overlaps ortho {ortho}"


def test_kitchen_sink_exo_does_not_cover_index_6():
    boxes = _label_boxes(_kitchen_sink_svg())
    six = boxes[("number", "6")]
    exo = boxes[("label", "exo")]
    assert not _overlaps(six, exo), f"index 6 {six} overlaps exo {exo}"
