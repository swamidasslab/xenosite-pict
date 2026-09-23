# Auto-generated from Rust schemars (make types) — do not edit.
"""EdgePlan / EdgeResult — live ABI from xpict-core (schemars)."""

from __future__ import annotations

from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field, RootModel


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")

    def model_dump(self, *args, **kwargs):
        # Match Rust ``skip_serializing_if = Option::is_none`` / empty skips.
        kwargs.setdefault("exclude_none", True)
        return super().model_dump(*args, **kwargs)

    def model_dump_json(self, *args, **kwargs):
        kwargs.setdefault("exclude_none", True)
        return super().model_dump_json(*args, **kwargs)


class AlignOpts(StrictModel):
    """Align this mol onto its parent template."""
    atom_map: list[tuple[int, int]] | None = Field(None, description='Pairs `(query_atom, template_atom)`. `None` → edge runs MCS.')
    min_atoms: int | None = Field(None, description='Override [`MIN_MCS_ATOMS`] when set.')


class AtomIn(StrictModel):
    """Atom input for a molecule depict call (SVG-space coords from the caller)."""
    index: int
    x: float
    y: float
    charge: int = 0
    element: str | None = None
    label: str | None = None
    z: int | None = Field(None, description='Atomic number (`0` = ``*``). Used when ``element`` is omitted.')


class BondIn(StrictModel):
    """Bond input (indices into [`AtomIn::index`])."""
    begin: int
    end: int
    index: int
    order: float
    interior: tuple[float, float] | None = Field(None, description='Unit normal toward ring interior (ring doubles); omit for centered.')
    stereo: str | None = None


class MoleculeIn(StrictModel):
    """One molecule ready to paint (coords already in SVG / ``SCALE`` space)."""
    atoms: list[AtomIn]
    bonds: list[BondIn]
    atom_shade: list[float] | None = Field(None, description='Per-atom shade scores (same order as [`Self::atoms`]); omit if unshaded.')
    bond_shade: list[float] | None = Field(None, description='Per-bond shade scores (same order as [`Self::bonds`]).')
    color: str | None = Field(None, description='Ink color for backbone bonds and atom labels (CSS). Default ``#111``.')
    id: str | None = None
    mark_atoms: list[int] = Field(default_factory=list, description='Atom indices to circle (publication marks).')
    mark_bonds: list[tuple[int, int]] = Field(default_factory=list, description='Bond endpoint index pairs to circle/stroke-mark.')
    scale: float | None = Field(None, description='Uniform diagram scale (font, stroke, pad, geometry). ``1.0`` = house size.')
    shade_vmax: float | None = Field(None, description='Shade colormap window high (default ``1``). Not inferred from data.')
    shade_vmin: float | None = Field(None, description='Shade colormap window low (default ``0``). Not inferred from data.')
    weight: float | None = Field(None, description='Ink weight for backbone stroke and label glyph thicken. ``1.0`` = house; may go down to ~``2/3`` (Regular stem); typical thicken up to ~2.')


class MolTemplate(StrictModel):
    """One node in a coord_gen forest (root = free layout; children align to parent)."""
    id: str = Field(description='Rust-assigned unique id; round-trips to the document node.')
    align: AlignOpts | None = Field(None, description='Opts for aligning onto the parent; `None` on roots.')
    cxsmiles: str | None = None
    molfile: str | None = None
    smiles: str | None = None
    template_for: list[MolTemplate] = Field(default_factory=list, description='Children that use this node as their align template.')


# How coords were produced for one molecule.
CoordMethod = Literal['free', 'atom_map', 'mcs', 'none']


class CoordGenTask(StrictModel):
    type: Literal['coord_gen'] = 'coord_gen'
    roots: list[MolTemplate] = Field(default_factory=list)


EdgeTask = CoordGenTask


class EdgePlan(StrictModel):
    """Host callback request."""
    version: int
    tasks: list[EdgeTask] = Field(default_factory=list)


class CoordGenMoleculeResult(StrictModel):
    """One flat molecule entry in a coord_gen result."""
    id: str
    method: CoordMethod
    ok: bool = Field(description='True when usable coords were produced (aligned or free-layout fallback).')
    error: str | None = None
    molecule: MoleculeIn | None = Field(None, description='Present whenever ``ok`` is true.')
    used_map: list[tuple[int, int]] | None = None


class CoordGenTaskResult(StrictModel):
    type: Literal['coord_gen'] = 'coord_gen'
    molecules: list[CoordGenMoleculeResult]
    ok: bool


EdgeTaskResult = CoordGenTaskResult


class EdgeResult(StrictModel):
    version: int
    results: list[EdgeTaskResult] = Field(default_factory=list)
