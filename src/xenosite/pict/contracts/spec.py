"""Declarative PictSpec — language-neutral input contract (Pydantic source of truth)."""

from __future__ import annotations

from enum import Enum
from typing import Annotated, Any, Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class DiagramKind(str, Enum):
    single = "single"
    grid = "grid"
    network = "network"
    reaction = "reaction"
    html = "html"


class MarkKind(str, Enum):
    atoms = "atoms"
    bonds = "bonds"
    substructure = "substructure"


class MarkSpec(StrictModel):
    """Publication-style annotation of atoms, bonds, or a substructure."""

    kind: MarkKind = MarkKind.atoms
    atoms: list[int] | None = Field(
        default=None, description="0-based atom indices to mark"
    )
    bonds: list[tuple[int, int]] | None = Field(
        default=None, description="0-based atom-index pairs for bonds to mark"
    )
    color: str | None = Field(default=None, description="CSS color for the mark")
    label: str | None = None


class ShadeSpec(StrictModel):
    """Continuous per-atom / per-bond scores for colormap shading."""

    atoms: list[float] | None = Field(
        default=None, description="Per-atom scores (same length as atom count)"
    )
    bonds: list[float] | None = Field(
        default=None, description="Per-bond scores"
    )
    colormap: str = Field(default="xenosite", description="Named colormap")
    vmin: float | None = None
    vmax: float | None = None


class MoleculeSpec(StrictModel):
    """One molecule in a depiction document."""

    id: str | None = Field(default=None, description="Stable id for diagram edges/nodes")
    smiles: str | None = Field(default=None, description="SMILES string")
    cxsmiles: str | None = Field(
        default=None,
        description="ChemAxon extended SMILES (CXSMILES), including coords/labels when present",
    )
    esmiles: str | None = Field(
        default=None,
        description="MolParser E-SMILES (SMILES<sep>EXTENSION); schema hook for Markush",
    )
    molfile: str | None = Field(default=None, description="Optional MDL molblock")
    title: str | None = None
    marks: list[MarkSpec] = Field(default_factory=list)
    shade: ShadeSpec | None = None
    halo: bool = True
    color: str | None = Field(default=None, description="Bond/backbone color override")

    @model_validator(mode="after")
    def require_structure(self) -> MoleculeSpec:
        if not any([self.smiles, self.cxsmiles, self.esmiles, self.molfile]):
            raise ValueError(
                "MoleculeSpec requires smiles, cxsmiles, esmiles, or molfile"
            )
        return self


class EdgeArrow(str, Enum):
    """Arrow head / shaft style for diagram edges."""

    forward = "forward"  # single →
    equilibrium = "equilibrium"  # ⇌ stacked half-arrows
    open = "open"  # ⇒ hollow head (retrosynthetic-style)
    line = "line"  # connector without arrowhead


class EdgeSpec(StrictModel):
    """Edge between molecule nodes in a network / reaction diagram."""

    source: str
    target: str
    label: str | None = None
    role: str | None = Field(
        default=None,
        description="Optional semantic role (e.g. enzyme, inhibits) — not drawn by default",
    )
    arrow: EdgeArrow = EdgeArrow.forward
    color: str | None = Field(default=None, description="Stroke/fill color for the arrow")
    stroke_width: float | None = Field(default=None, description="Shaft stroke width")
    dashed: bool = Field(default=False, description="Dashed shaft (e.g. hypothetical step)")


class DiagramSpec(StrictModel):
    """Multi-molecule composition options (ELK / grid / HTML)."""

    kind: DiagramKind = DiagramKind.single
    columns: int | None = Field(default=None, description="Grid columns when kind=grid")
    edges: list[EdgeSpec] = Field(default_factory=list)
    elk_options: dict[str, Any] = Field(
        default_factory=dict,
        description="Pass-through ELK layout options (engine-specific support varies)",
    )
    align: bool = Field(
        default=False,
        description="Optional chemical MCS alignment before diagram layout",
    )


class PictSpec(StrictModel):
    """Declarative depiction document.

    Engine choice (`backend`) is runtime config on Pict/render — not part of this document.
    """

    molecules: Annotated[list[MoleculeSpec], Field(min_length=1)]
    diagram: DiagramSpec = Field(default_factory=DiagramSpec)
    width: float | None = Field(default=None, description="Preferred SVG width")
    height: float | None = Field(default=None, description="Preferred SVG height")
    meta: dict[str, Any] = Field(default_factory=dict)
