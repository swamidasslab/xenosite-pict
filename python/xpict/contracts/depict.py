"""Live declarative subset — mol list in → list of rendered molecules out.

Matches JS ``xpict.depict`` / Rust ``xpict::depict``. This is what the public
package ships today. Nested diagrams, annotations, ELK, etc. stay in
``xpict.future``.
"""

from __future__ import annotations

from pydantic import BaseModel, ConfigDict, Field, model_validator


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class MolSpec(StrictModel):
    """One molecule entry for the batch ``depict`` stub (paint options only).

    Structure: pass exactly one of ``smiles``, ``cxsmiles``, ``molfile``, or
    ``source`` (alias for any structure string).

    Alignment is **not** on this model — use the client ``render(..., align_to=)``
    with a Mol / Rendered target (or Rust pose molblock).
    """

    smiles: str | None = Field(default=None, description="SMILES string")
    cxsmiles: str | None = Field(
        default=None,
        description="ChemAxon extended SMILES (CXSMILES)",
    )
    molfile: str | None = Field(default=None, description="MDL molblock")
    source: str | None = Field(
        default=None,
        description="Structure alias (SMILES / CXSMILES / molfile text)",
    )
    id: str | None = Field(default=None, description="Optional molecule id")
    color: str | None = Field(
        default=None,
        description="Ink color for backbone bonds and atom labels (CSS)",
    )
    atom_shade: list[float] | None = Field(
        default=None,
        description="Per-atom shade scores (layout encounter order)",
    )
    bond_shade: list[float] | None = Field(
        default=None,
        description="Per-bond shade scores (layout encounter order)",
    )
    mark_atoms: list[int] | None = Field(
        default=None,
        description="Atom indices to circle (publication marks)",
    )
    mark_bonds: list[tuple[int, int]] | None = Field(
        default=None,
        description="Bond endpoint index pairs to mark",
    )
    star_labels: list[str | None] | None = Field(
        default=None,
        description=(
            "Labels for ``*`` atoms in encounter order; "
            "null/empty → bare star. When omitted, CXSMILES ``|$…$|`` "
            "aliases apply by atom index."
        ),
    )
    bold_labels: bool | None = Field(
        default=None,
        description="Bold Liberation labels + stem-keyed bond stroke",
    )

    @model_validator(mode="after")
    def _need_structure(self) -> MolSpec:
        for field in (self.smiles, self.cxsmiles, self.molfile, self.source):
            if field is not None and str(field).strip():
                return self
        raise ValueError("MolSpec needs smiles, cxsmiles, molfile, or source")


class DepictSpec(StrictModel):
    """Batch document: ordered mols → ordered rendered results."""

    molecules: list[MolSpec] = Field(default_factory=list)
