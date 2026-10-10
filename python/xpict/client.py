"""Single-molecule client types — parity with JS ``xpict.mol`` / ``toSvg``.

Layout + align belong in CoordGen / Rust (see ``layout2d`` and ``align``).
``render()`` awaits a native SMILES→MoleculeIn bridge.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from xpict.contracts.scene import Scene
from xpict.draw.svg import scene_to_svg as _scene_to_svg


@dataclass
class Mol:
    """Input molecule — SMILES / CXSMILES / molfile plus optional align frame."""

    source: str
    frame_molblock: str | None = None

    def render(self, opts: MolRenderOptions | None = None) -> Rendered:
        return render(self, opts)

    def ensure_frame(self) -> str:
        raise NotImplementedError(
            "Mol.ensure_frame requires the native CoordGen SMILES bridge."
        )


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
class Rendered:
    """Painted depiction — editable ``scene`` plus alignment frame."""

    width: float
    height: float
    scene: Scene
    molecule: dict[str, Any]
    source: str
    frame_molblock: str
    coords: list[SvgAtom]
    svg_coords: list[SvgAtom]
    bonds: list[SvgBond]
    mol: Mol | None = None

    def to_svg(self) -> str:
        return to_svg(self.scene)

    def frame(self) -> str:
        return self.frame_molblock


@dataclass
class MolRenderOptions:
    """Render options — parity with JS / Rust ``MolRenderOptions``."""

    id: str | None = None
    color: str | None = None
    atom_shade: list[float] | None = None
    bond_shade: list[float] | None = None
    mark_atoms: list[int] | None = None
    mark_bonds: list[tuple[int, int]] | None = None
    star_labels: list[str | None] | None = None
    weight: float | None = None
    scale: float | None = None
    align_to: Mol | Rendered | None = None
    atom_map: list[tuple[int, int]] | None = None


def mol(smiles_or_molfile: str) -> Mol:
    """Construct a ``Mol`` (JS / Rust ``xpict.mol``)."""
    text = smiles_or_molfile.strip()
    if not text:
        raise ValueError("mol() requires a non-empty SMILES or molfile")
    return Mol(source=text)


def to_svg(scene: Scene | dict[str, Any]) -> str:
    """Scene JSON / ``Scene`` → SVG string (JS ``xpict.toSvg``)."""
    if isinstance(scene, dict):
        scene = Scene.model_validate(scene)
    return _scene_to_svg(scene)


def render(
    input: Mol | str,
    opts: MolRenderOptions | dict[str, Any] | None = None,
) -> Rendered:
    """Layout → native ``depict_molecule`` → ``Rendered``.

    Python host layout was removed. Use ``layout2d`` + ``_native.depict_molecule``
    until the CoordGen SMILES bridge lands, or the Rust/JS client.
    """
    _ = input, opts
    raise NotImplementedError(
        "Python mol()/render() layout was removed. "
        "Use layout2d.generate_2d_coordinates and _native.depict_molecule, "
        "or the Rust/JS client."
    )
