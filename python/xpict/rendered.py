"""Packed paint row + thin Scene→SVG (Rust `scene_to_svg`; host is IPython/HTML)."""

from __future__ import annotations

import html
import json
import re
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, Sequence
from urllib.parse import quote

from xpict.base import PictBase
from xpict.contracts.depict import AlignCoords, AlignMode, AlignToSpec
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
    radical: int = 0


@dataclass
class SvgBond:
    index: int
    begin: int
    end: int
    order: float
    stereo: str | None = None


def _frame_json_to_molblock(frame: str) -> str:
    """CoordGen-space frame JSON → V2000 CTAB (pose carrier for ``align_to.molblock``)."""
    data = json.loads(frame)
    atoms = sorted(data["atoms"], key=lambda a: int(a.get("index", 0)))
    bonds = data.get("bonds") or []
    n = len(atoms)
    nb = len(bonds)
    lines = ["", "  xpict", "", f"{n:>3}{nb:>3}  0  0  0  0  0  0  0  0999 V2000"]
    for a in atoms:
        sym = str(a.get("element") or "C")
        if len(sym) > 3:
            sym = sym[:3]
        lines.append(
            f"{float(a.get('x', 0)):10.4f}{float(a.get('y', 0)):10.4f}"
            f"{0.0:10.4f} {sym:<3} 0  0  0  0  0  0  0  0  0  0  0  0"
        )
    for b in bonds:
        order = int(round(float(b.get("order", 1))))
        order = max(1, min(4, order))
        lines.append(
            f"{int(b.get('begin', 0)) + 1:>3}{int(b.get('end', 0)) + 1:>3}{order:>3}  0  0  0  0"
        )
    chg = [
        (i + 1, int(a.get("charge", 0)))
        for i, a in enumerate(atoms)
        if int(a.get("charge", 0)) != 0
    ]
    for i in range(0, len(chg), 8):
        chunk = chg[i : i + 8]
        parts = [f"M  CHG{len(chunk):>3}"]
        for idx, c in chunk:
            parts.append(f"{idx:>4}{c:>4}")
        lines.append("".join(parts))
    lines.append("M  END")
    return "\n".join(lines) + "\n"


def align_from_frame(
    frame_molblock: str,
    *,
    atom_map: Sequence[tuple[int, int]] | None = None,
    min_atoms: int | None = None,
    mode: AlignMode | None = None,
    timeout_ms: int | None = None,
    **_extra: Any,
) -> AlignToSpec:
    """``AlignToSpec`` from CoordGen-space frame JSON (host helper core)."""
    if _extra:
        raise TypeError(f"align_from_frame() unexpected kwargs: {sorted(_extra)}")
    if atom_map is None:
        return AlignToSpec(
            molblock=_frame_json_to_molblock(frame_molblock),
            min_atoms=min_atoms,
            mode=mode,
            timeout_ms=timeout_ms,
        )
    # Remap: atom_map pairs are (query, template); coords use query indices
    # with CoordGen-space positions from frame (not paint coords).
    frame = json.loads(frame_molblock)
    by_index = {int(a["index"]): a for a in frame["atoms"]}
    xs: list[float] = []
    ys: list[float] = []
    atoms: list[int] = []
    for q, t in atom_map:
        tmpl = by_index.get(int(t))
        if tmpl is None:
            raise ValueError(f"align_from atom_map: template atom {t} missing on cell")
        atoms.append(int(q))
        xs.append(float(tmpl["x"]))
        ys.append(float(tmpl["y"]))
    return AlignToSpec(
        coords=AlignCoords(x=xs, y=ys, atom=atoms),
        atom_map=[(int(q), int(t)) for q, t in atom_map],
        min_atoms=min_atoms,
        mode=mode,
        timeout_ms=timeout_ms,
    )


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
    id: str | None = None

    def to_svg(self) -> str:
        return scene_to_svg(self.scene)

    def frame(self) -> str:
        return self.frame_molblock

    def align_from(
        self,
        *,
        atom_map: Sequence[tuple[int, int]] | None = None,
        min_atoms: int | None = None,
        mode: AlignMode | None = None,
        timeout_ms: int | None = None,
        **_extra: Any,
    ) -> AlignToSpec:
        """``AlignToSpec`` constructor from this cell's CoordGen pose.

        Default (no ``atom_map``): ``{ molblock, … }`` for MCS.
        With ``atom_map``: ``{ coords, atom_map, … }`` remapped to **query**
        indices (soft-seed; no MCS).
        """
        return align_from_frame(
            self.frame_molblock,
            atom_map=atom_map,
            min_atoms=min_atoms,
            mode=mode,
            timeout_ms=timeout_ms,
            **_extra,
        )


def align_from(
    rows: Rendered | Sequence[Rendered],
    id: str | None = None,
    *,
    atom_map: Sequence[tuple[int, int]] | None = None,
    min_atoms: int | None = None,
    mode: AlignMode | None = None,
    timeout_ms: int | None = None,
) -> AlignToSpec:
    """Build a drop-in ``align_to`` from a prior cell (module sugar for list+id)."""
    row: Rendered
    if isinstance(rows, Rendered):
        row = rows
    else:
        seq = list(rows)
        if id is None:
            if len(seq) != 1:
                raise ValueError("align_from(rows) needs id= when len(rows) != 1")
            row = seq[0]
        else:
            matched = [r for r in seq if (r.id or r.molecule.id) == id]
            if len(matched) != 1:
                raise ValueError(f"align_from: expected one row with id={id!r}, got {len(matched)}")
            row = matched[0]
    return row.align_from(
        atom_map=atom_map,
        min_atoms=min_atoms,
        mode=mode,
        timeout_ms=timeout_ms,
    )

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
