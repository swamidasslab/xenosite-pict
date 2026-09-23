"""EdgePlan / EdgeResult — host callback ABI (coord_gen / align).

Rust core builds the plan (ids + forest); language edges process it and return
flat ``MoleculeIn`` results. No molblocks on this wire.
"""

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class AlignOpts(StrictModel):
    """Align this mol onto its parent template (``MolTemplate.align``)."""

    atom_map: list[tuple[int, int]] | None = Field(
        default=None,
        description=(
            "Pairs (query_atom, template_atom). Omit → edge runs MCS. "
            "Present → skip MCS and use this map."
        ),
    )
    min_atoms: int | None = Field(
        default=None,
        ge=1,
        description="Minimum mapped atoms (default 3 = MIN_MCS_ATOMS).",
    )


class MolTemplate(StrictModel):
    """One node in a coord_gen forest (root = free layout; children align to parent)."""

    id: str = Field(description="Rust-assigned unique id; round-trips to the doc node")
    smiles: str | None = None
    cxsmiles: str | None = None
    molfile: str | None = None
    align: AlignOpts | None = Field(
        default=None,
        description="Opts for aligning onto the parent; null on roots",
    )
    template_for: list[MolTemplate] = Field(
        default_factory=list,
        description="Children that use this node as their align template",
    )

    @model_validator(mode="after")
    def _one_structure(self) -> MolTemplate:
        n = sum(
            1
            for v in (self.smiles, self.cxsmiles, self.molfile)
            if v is not None and str(v).strip()
        )
        if n != 1:
            raise ValueError("MolTemplate needs exactly one of smiles/cxsmiles/molfile")
        return self


class CoordGenTask(StrictModel):
    type: Literal["coord_gen"] = "coord_gen"
    roots: list[MolTemplate] = Field(
        description="Independent align trees (each root laid out freely)",
    )


EdgeTask = CoordGenTask


class EdgePlan(StrictModel):
    """Host callback request — today only ``coord_gen``; more task types later."""

    version: Literal[1] = 1
    tasks: list[CoordGenTask] = Field(default_factory=list)


class CoordGenMoleculeResult(StrictModel):
    id: str
    ok: bool = Field(
        description=(
            "True when usable coords were produced (aligned or free-layout fallback). "
            "False only when even unaligned coord gen failed."
        ),
    )
    method: Literal["free", "atom_map", "mcs", "none"] = Field(
        description=(
            "free = unconstrained; atom_map / mcs = aligned; "
            "none = align requested but failed — automatic free-layout fallback"
        ),
    )
    used_map: list[tuple[int, int]] | None = Field(
        default=None,
        description="(query, template) pairs when a map was applied",
    )
    molecule: dict[str, Any] | None = Field(
        default=None,
        description="MoleculeIn JSON (atoms/bonds/coords); present whenever ok is true",
    )
    error: str | None = Field(
        default=None,
        description="Note when method is none (fallback) or ok is false",
    )


class CoordGenTaskResult(StrictModel):
    type: Literal["coord_gen"] = "coord_gen"
    ok: bool = Field(
        description="True when every molecule entry has ok=true (coords for all ids)",
    )
    molecules: list[CoordGenMoleculeResult] = Field(
        description="Flat list — align failures still appear with free-layout coords",
    )


class EdgeResult(StrictModel):
    version: Literal[1] = 1
    results: list[CoordGenTaskResult] = Field(default_factory=list)
