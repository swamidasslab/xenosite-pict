"""Declarative document — strict subset of ``xpict.future`` PictSpec.

Public document shape matches the nested future tree, trimmed to what paint
supports today:

- Root is a ``type: "mol"`` leaf, or a ``type: "group"`` with ``children``
- Molecule discriminator is ``type: "mol"``
- Shade via ``shade: {atoms, bonds, …}``
- Markush / star text via ``star_labels`` (encounter order) or CXSMILES
  aliases (e.g. ``|$R1;;;;;$|``). Document ``rgroups`` stays in future.

Everything here must validate as :class:`~xpict.future.nodes.PictSpec`.
Richer nodes (reaction, annotations, ``rgroups``, …) stay in
``xpict.future`` until they graduate.
"""

from __future__ import annotations

from typing import Annotated, Literal

from pydantic import (
    BaseModel,
    ConfigDict,
    Field,
    RootModel,
    model_validator,
)

from xpict.future.spec import ShadeSpec


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class MolNode(StrictModel):
    """Molecule node — subset of future ``MolNode`` / ``MoleculeSpec``."""

    type: Literal["mol"] = "mol"
    id: str | None = Field(default=None, description="Optional stable id")
    smiles: str | None = Field(default=None, description="SMILES string")
    cxsmiles: str | None = Field(
        default=None,
        description="ChemAxon extended SMILES (CXSMILES)",
    )
    molfile: str | None = Field(default=None, description="MDL molblock")
    color: str | None = Field(
        default=None,
        description="Ink color for backbone bonds and atom labels (CSS)",
    )
    shade: ShadeSpec | None = Field(
        default=None,
        description="Per-atom / per-bond colormap scores",
    )
    star_labels: list[str | None] | None = Field(
        default=None,
        description=(
            "Labels for ``*`` atoms in layout encounter order "
            "(``null`` / empty → bare *; chem markup OK). "
            "Wins over CXSMILES aliases when both are present."
        ),
    )
    scale: float = Field(
        default=1.0,
        description="Uniform diagram scale (font, stroke, geometry). 1.0 = house size.",
        gt=0,
    )
    weight: float = Field(
        default=1.0,
        description=(
            "Ink weight relative to house size (1.0). "
            "May go down to 2/3 (Regular stem); typical thicken up to ~2."
        ),
        ge=2.0 / 3.0,
    )
    align_to: str | None = Field(
        default=None,
        description=(
            "Id of another mol in this group to use as align template. "
            "When omitted and the group has align=true, defaults to the first child."
        ),
    )
    atom_map: list[tuple[int, int]] | None = Field(
        default=None,
        description=(
            "Pairs (query_atom, template_atom) vs the implicit / align_to template. "
            "Skips MCS when set."
        ),
    )

    @model_validator(mode="after")
    def _need_structure(self) -> MolNode:
        for field in (self.smiles, self.cxsmiles, self.molfile):
            if field is not None and str(field).strip():
                return self
        raise ValueError("mol node needs smiles, cxsmiles, or molfile")


class GroupNode(StrictModel):
    """Group — subset of future ``GroupNode`` (mol children only today)."""

    type: Literal["group"] = "group"
    id: str | None = Field(default=None, description="Optional stable id")
    align: bool = Field(
        default=False,
        description=(
            "When true, later children align onto the first (or each child's "
            "align_to id). Builds an EdgePlan coord_gen forest."
        ),
    )
    children: list[MolNode] = Field(
        default_factory=list,
        description="Owned mol nodes (discriminated by ``type``)",
    )


DepictRoot = Annotated[MolNode | GroupNode, Field(discriminator="type")]


class DepictSpec(RootModel[DepictRoot]):
    """Declarative document (strict subset of future PictSpec)."""

    def mols(self) -> list[MolNode]:
        root = self.root
        if isinstance(root, MolNode):
            return [root]
        return list(root.children)


# Alias for MolNode.
MolSpec = MolNode
