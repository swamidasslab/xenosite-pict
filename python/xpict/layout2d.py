"""2D coordinate generation (Rust → private CoordGen backend).

Chemistry-facing API — no CoordGen types leak here. Prefer
:func:`generate_2d_coordinates` with :class:`Layout2DRequest`.

Flags map to real ``sketcherMinimizer`` setters / precision (see module
constants). Draft options that do not exist upstream (extra boxes,
template_strength, collision_score) are not exposed.
"""

from __future__ import annotations

from typing import Literal, Sequence, overload

from pydantic import BaseModel, ConfigDict, Field, model_validator

from xpict import _native as _rust

LAYOUT_FLAG_EVEN_ANGLES: int = int(getattr(_rust, "LAYOUT_FLAG_EVEN_ANGLES", 1 << 0))
LAYOUT_FLAG_QUICK_PRECISION: int = int(
    getattr(_rust, "LAYOUT_FLAG_QUICK_PRECISION", 1 << 1)
)
LAYOUT_FLAG_FORCE_OPEN_MACROCYCLES: int = int(
    getattr(_rust, "LAYOUT_FLAG_FORCE_OPEN_MACROCYCLES", 1 << 2)
)
LAYOUT_FLAG_BEST_PRECISION: int = int(
    getattr(_rust, "LAYOUT_FLAG_BEST_PRECISION", 1 << 3)
)
LAYOUT_FLAG_METAL_BONDS_AS_ZOB: int = int(
    getattr(_rust, "LAYOUT_FLAG_METAL_BONDS_AS_ZOB", 1 << 4)
)
LAYOUT_FLAG_SKIP_MINIMIZATION: int = int(
    getattr(_rust, "LAYOUT_FLAG_SKIP_MINIMIZATION", 1 << 5)
)
# Draft aliases
LAYOUT_FLAG_FORCE_PLANAR: int = LAYOUT_FLAG_EVEN_ANGLES
LAYOUT_FLAG_MACROCYCLE_MODE: int = LAYOUT_FLAG_FORCE_OPEN_MACROCYCLES

BOND_STEREO_NONE: int = int(getattr(_rust, "BOND_STEREO_NONE", 0))
BOND_STEREO_CIS: int = int(getattr(_rust, "BOND_STEREO_CIS", 1))
BOND_STEREO_TRANS: int = int(getattr(_rust, "BOND_STEREO_TRANS", 2))

BondStereo = Literal["none", "cis", "trans", 0, 1, 2]


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class Layout2DAtom(StrictModel):
    """One atom for free / constrained 2D layout."""

    atomic_number: int = Field(ge=0, le=255)
    formal_charge: int = 0
    #: Seed coordinates when ``constrained`` or ``fixed``.
    x: float | None = None
    y: float | None = None
    #: Soft template constraint (``atom->constrained``).
    constrained: bool = False
    #: Hard fix at ``x``/``y`` (``atom->fixed``).
    fixed: bool = False

    @model_validator(mode="after")
    def _coords_when_constrained(self) -> Layout2DAtom:
        if (self.constrained or self.fixed) and (self.x is None or self.y is None):
            raise ValueError("constrained/fixed atoms require x and y")
        return self


class Layout2DBond(StrictModel):
    begin: int = Field(ge=0)
    end: int = Field(ge=0)
    order: int = Field(default=1, ge=0, le=3)
    stereo: BondStereo = "none"
    #: Substituent atom indices for CIP-aware cis/trans (recommended for doubles).
    stereo_atom1: int | None = None
    stereo_atom2: int | None = None

    @model_validator(mode="after")
    def _stereo_pair(self) -> Layout2DBond:
        a1, a2 = self.stereo_atom1, self.stereo_atom2
        if (a1 is None) ^ (a2 is None):
            raise ValueError("stereo_atom1 and stereo_atom2 must both be set or both omitted")
        return self


class Layout2DRequest(StrictModel):
    """Full layout request (atoms + bonds + real CoordGen flags)."""

    atoms: list[Layout2DAtom] = Field(min_length=1)
    bonds: list[Layout2DBond] = Field(default_factory=list)
    #: Bitmask of ``LAYOUT_FLAG_*`` (even angles, precision, macrocycles, …).
    layout_flags: int = 0

    @model_validator(mode="after")
    def _bond_indices(self) -> Layout2DRequest:
        n = len(self.atoms)
        for b in self.bonds:
            if b.begin >= n or b.end >= n:
                raise ValueError(f"bond {b.begin}-{b.end} out of range for {n} atoms")
            if b.stereo_atom1 is not None and (
                b.stereo_atom1 >= n or b.stereo_atom2 is not None and b.stereo_atom2 >= n
            ):
                raise ValueError("stereo atom index out of range")
        return self


class Layout2DResult(StrictModel):
    x: list[float]
    y: list[float]

    @property
    def coords(self) -> list[tuple[float, float]]:
        return list(zip(self.x, self.y, strict=True))


def _stereo_code(stereo: BondStereo) -> int:
    if stereo in ("none", 0, BOND_STEREO_NONE):
        return BOND_STEREO_NONE
    if stereo in ("cis", 1, BOND_STEREO_CIS):
        return BOND_STEREO_CIS
    if stereo in ("trans", 2, BOND_STEREO_TRANS):
        return BOND_STEREO_TRANS
    raise ValueError(f"unknown bond stereo: {stereo!r}")


def _bools_to_flags(flags: Sequence[bool]) -> list[int]:
    return [1 if f else 0 for f in flags]


@overload
def generate_2d_coordinates(request: Layout2DRequest) -> Layout2DResult: ...


@overload
def generate_2d_coordinates(
    atomic_numbers: Sequence[int],
    bond_from: Sequence[int],
    bond_to: Sequence[int],
    bond_orders: Sequence[int],
    *,
    formal_charges: Sequence[int] | None = None,
    bond_stereos: Sequence[BondStereo] | None = None,
    bond_stereo_atom1: Sequence[int] | None = None,
    bond_stereo_atom2: Sequence[int] | None = None,
    has_template_match: Sequence[bool] | None = None,
    is_atom_fixed: Sequence[bool] | None = None,
    input_x: Sequence[float] | None = None,
    input_y: Sequence[float] | None = None,
    layout_flags: int = 0,
) -> Layout2DResult: ...


def generate_2d_coordinates(
    atomic_numbers: Layout2DRequest | Sequence[int],
    bond_from: Sequence[int] | None = None,
    bond_to: Sequence[int] | None = None,
    bond_orders: Sequence[int] | None = None,
    *,
    formal_charges: Sequence[int] | None = None,
    bond_stereos: Sequence[BondStereo] | None = None,
    bond_stereo_atom1: Sequence[int] | None = None,
    bond_stereo_atom2: Sequence[int] | None = None,
    has_template_match: Sequence[bool] | None = None,
    is_atom_fixed: Sequence[bool] | None = None,
    input_x: Sequence[float] | None = None,
    input_y: Sequence[float] | None = None,
    layout_flags: int = 0,
) -> Layout2DResult:
    """Generate 2D coordinates in Rust (static CoordGen); Python only receives results."""
    if not getattr(_rust, "HAS_COORDGEN", False):
        raise RuntimeError("xpict was built without the coordgen feature")

    if isinstance(atomic_numbers, Layout2DRequest):
        req = atomic_numbers
        an = [a.atomic_number for a in req.atoms]
        fc = [a.formal_charge for a in req.atoms]
        bf = [b.begin for b in req.bonds]
        bt = [b.end for b in req.bonds]
        bo = [b.order for b in req.bonds]
        bs = [_stereo_code(b.stereo) for b in req.bonds]
        s1 = [b.stereo_atom1 if b.stereo_atom1 is not None else -1 for b in req.bonds]
        s2 = [b.stereo_atom2 if b.stereo_atom2 is not None else -1 for b in req.bonds]
        have_stereo_atoms = any(b.stereo_atom1 is not None for b in req.bonds)
        templ = _bools_to_flags([a.constrained for a in req.atoms])
        fixed = _bools_to_flags([a.fixed for a in req.atoms])
        ix = [float(a.x) if a.x is not None else 0.0 for a in req.atoms]
        iy = [float(a.y) if a.y is not None else 0.0 for a in req.atoms]
        use_xy = any(a.constrained or a.fixed for a in req.atoms)
        native = _rust.generate_2d_coordinates(
            an,
            bf,
            bt,
            bo,
            formal_charges=fc,
            bond_stereos=bs,
            bond_stereo_atom1=s1 if have_stereo_atoms else None,
            bond_stereo_atom2=s2 if have_stereo_atoms else None,
            has_template_match=templ if any(templ) else None,
            is_atom_fixed=fixed if any(fixed) else None,
            input_x=ix if use_xy else None,
            input_y=iy if use_xy else None,
            layout_flags=req.layout_flags,
        )
    else:
        if bond_from is None or bond_to is None or bond_orders is None:
            raise TypeError(
                "generate_2d_coordinates requires bond_from/bond_to/bond_orders "
                "or a Layout2DRequest"
            )
        stereo_i = (
            [_stereo_code(s) for s in bond_stereos] if bond_stereos is not None else None
        )
        native = _rust.generate_2d_coordinates(
            list(atomic_numbers),
            list(bond_from),
            list(bond_to),
            list(bond_orders),
            formal_charges=list(formal_charges) if formal_charges is not None else None,
            bond_stereos=stereo_i,
            bond_stereo_atom1=(
                list(bond_stereo_atom1) if bond_stereo_atom1 is not None else None
            ),
            bond_stereo_atom2=(
                list(bond_stereo_atom2) if bond_stereo_atom2 is not None else None
            ),
            has_template_match=(
                _bools_to_flags(has_template_match)
                if has_template_match is not None
                else None
            ),
            is_atom_fixed=(
                _bools_to_flags(is_atom_fixed) if is_atom_fixed is not None else None
            ),
            input_x=list(input_x) if input_x is not None else None,
            input_y=list(input_y) if input_y is not None else None,
            layout_flags=layout_flags,
        )

    return Layout2DResult(
        x=[float(v) for v in native.x],
        y=[float(v) for v in native.y],
    )


__all__ = [
    "BOND_STEREO_CIS",
    "BOND_STEREO_NONE",
    "BOND_STEREO_TRANS",
    "LAYOUT_FLAG_BEST_PRECISION",
    "LAYOUT_FLAG_EVEN_ANGLES",
    "LAYOUT_FLAG_FORCE_OPEN_MACROCYCLES",
    "LAYOUT_FLAG_FORCE_PLANAR",
    "LAYOUT_FLAG_MACROCYCLE_MODE",
    "LAYOUT_FLAG_METAL_BONDS_AS_ZOB",
    "LAYOUT_FLAG_QUICK_PRECISION",
    "LAYOUT_FLAG_SKIP_MINIMIZATION",
    "BondStereo",
    "Layout2DAtom",
    "Layout2DBond",
    "Layout2DRequest",
    "Layout2DResult",
    "generate_2d_coordinates",
]
