"""Backend-agnostic layout result contract."""

from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class AtomLayout(StrictModel):
    index: int
    element: str
    x: float
    y: float
    charge: int = 0
    isotope: int | None = None
    radical: int = Field(
        default=0,
        description="Unpaired electron count (0 = closed shell; 1 = doublet radical)",
    )
    label: str | None = Field(
        default=None,
        description="Display label; None means suppress (e.g. carbon). "
        "Stars / R-groups use '*' or a CXSMILES alias (e.g. R1).",
    )


class BondLayout(StrictModel):
    index: int
    begin: int
    end: int
    order: float = 1.0
    stereo: Literal["up", "down", "either", "none"] | None = None


class MoleculeLayout(StrictModel):
    id: str | None = None
    atoms: list[AtomLayout]
    bonds: list[BondLayout]
    backend: str
    warnings: list[str] = Field(default_factory=list)


class LayoutResult(StrictModel):
    """One or more laid-out molecules ready for scene construction."""

    molecules: list[MoleculeLayout]
