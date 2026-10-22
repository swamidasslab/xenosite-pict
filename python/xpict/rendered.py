"""Packed paint row + thin Scene→SVG (Rust `scene_to_svg`; host is IPython/HTML)."""

from __future__ import annotations

import html
import re
from dataclasses import dataclass
from typing import TYPE_CHECKING
from urllib.parse import quote

from xpict.base import PictBase
from xpict.contracts.edge import MoleculeIn
from xpict.contracts.scene import Scene
from xpict.contracts.scene_dict import SceneDict
from xpict.native_bridge import scene_to_svg as _native_scene_to_svg

if TYPE_CHECKING:
    from xpict.spec import Mol

_SVG_ROOT_ATTR = re.compile(r"<svg\b([^>]*)>", re.IGNORECASE | re.DOTALL)
_SVG_ATTR = re.compile(r'([\w:-]+)="([^"]*)"')


def _strip_xml_decl(svg: str) -> str:
    if svg.startswith("<?xml"):
        return svg.split("\n", 1)[1] if "\n" in svg else svg.split("?>", 1)[-1]
    return svg


def svg_root_size_px(svg: str) -> tuple[str, str] | None:
    m = _SVG_ROOT_ATTR.search(svg)
    if not m:
        return None
    attrs = dict(_SVG_ATTR.findall(m.group(1)))
    w = attrs.get("width", "").removesuffix("px")
    h = attrs.get("height", "").removesuffix("px")
    if not w or not h:
        return None
    return w, h


def svg_to_data_uri(svg: str) -> str:
    body = _strip_xml_decl(svg).strip()
    return "data:image/svg+xml;utf8," + quote(body, safe="")


def svg_to_img_tag(
    svg: str,
    *,
    alt: str = "xpict",
    cls: str = "xpict",
) -> str:
    uri = svg_to_data_uri(svg)
    size = svg_root_size_px(svg)
    size_attrs = f' width="{size[0]}" height="{size[1]}"' if size else ""
    return (
        f'<img class="{html.escape(cls)}" src="{uri}"{size_attrs} '
        f'alt="{html.escape(alt)}" decoding="async" draggable="false"/>'
    )


def scene_to_svg(scene: Scene | SceneDict | str) -> str:
    """Scene JSON → SVG (Rust ``scene_to_svg``; JS ``xpict.toSvg``)."""
    return _native_scene_to_svg(scene)


def scene_to_html(scene: Scene, *, title: str | None = None) -> str:
    t = title or "xpict"
    img = svg_to_img_tag(scene_to_svg(scene), alt=t)
    te = html.escape(t)
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>{te}</title>
<style>
  body {{ margin: 0; font-family: system-ui, sans-serif; background: #fafafa; color: #111; }}
  .xpict-page {{ padding: 1rem; box-sizing: border-box; }}
  .xpict-mol {{
    position: relative;
    display: inline-block;
    max-width: 100%;
    line-height: 0;
  }}
  .xpict-mol img.xpict {{
    display: block;
    max-width: 100%;
    max-height: 100%;
    height: auto;
  }}
  .xpict-row {{
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 0.35rem;
  }}
</style>
</head>
<body>
<main class="xpict-page">
<div class="xpict-mol">
{img}
</div>
</main>
</body>
</html>
"""


def to_svg(scene: Scene | SceneDict) -> str:
    """Scene JSON / ``Scene`` → SVG string (JS ``xpict.toSvg``)."""
    if isinstance(scene, dict):
        scene = Scene.model_validate(scene)
    return scene_to_svg(scene)


@dataclass
class SvgAtom:
    index: int
    element: str
    x: float
    y: float
    label: str | None = None
    charge: int = 0


@dataclass
class SvgBond:
    index: int
    begin: int
    end: int
    order: float
    stereo: str | None = None


@dataclass
class Rendered(PictBase):
    """Painted depiction — editable ``scene`` plus alignment frame."""

    width: float
    height: float
    scene: Scene
    molecule: MoleculeIn
    source: str
    frame_molblock: str
    coords: list[SvgAtom]
    svg_coords: list[SvgAtom]
    bonds: list[SvgBond]
    mol: Mol | None = None

    def to_svg(self) -> str:
        return scene_to_svg(self.scene)

    def frame(self) -> str:
        return self.frame_molblock

    def to_spec(self):
        from xpict.spec import Mol

        if self.mol is not None:
            return self.mol.to_spec()
        return Mol(source=self.source, id=self.molecule.id).to_spec()

    def _repr_html_(self) -> str:
        svg = self.to_svg()
        return f'<div style="display:inline-block;line-height:0">{svg}</div>'

    def _repr_svg_(self) -> str:
        return self.to_svg()

    def _repr_mimebundle_(
        self,
        include: list[str] | None = None,
        exclude: list[str] | None = None,
    ) -> dict[str, str]:
        _ = include, exclude
        return {
            "text/html": self._repr_html_(),
            "image/svg+xml": self.to_svg(),
            "text/plain": f"Rendered({self.width:.0f}×{self.height:.0f}, {self.source!r})",
        }
