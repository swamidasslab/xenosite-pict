"""Packed paint row host object (Rust ``Rendered`` JSON → Python)."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from xpict.base import PictBase
from xpict.contracts.edge import MoleculeIn
from xpict.contracts.scene import Scene
from xpict.draw.svg import scene_to_svg

if TYPE_CHECKING:
    from xpict.client import Mol


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
        from xpict.contracts.depict import DepictSpec as WireDepictSpec
        from xpict.contracts.depict import MolNode
        from xpict.mol_source import source_to_mol_fields

        src = self.mol.source if self.mol is not None else self.source
        nid = self.mol.id if self.mol is not None else None
        return WireDepictSpec(
            MolNode.model_validate({"id": nid, **source_to_mol_fields(src)})
        )

    def _repr_html_(self) -> str:
        svg = self.to_svg()
        return f'<div style="display:inline-block;line-height:0">{svg}</div>'

    def _repr_svg_(self) -> str:
        return self.to_svg()
