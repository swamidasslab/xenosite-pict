"""Declarative document — strict subset of ``xpict.future`` PictSpec.

Public document shape matches the nested future tree, trimmed to what paint
supports today:

- Root is a ``type: "mol"`` leaf, or a ``type: "group"`` with ``children``
- Molecule discriminator is ``type: "mol"``
- Shade via ``shade: {atoms, bonds, …}``
- Markush / star text via CXSMILES aliases (e.g. ``|$R1;;;;;$|``) or the
  single-mol client's ``star_labels`` — not a document ``rgroups`` key yet

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
