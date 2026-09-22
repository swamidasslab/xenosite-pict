"""Live declarative subset — preferred document: mol list → rendered molecules.

Matches JS ``xpict.depict`` / Rust ``xpict::depict``. The simple
``mol`` / ``render`` / ``to_svg`` client is for single-mol callers; the
document path uses that layer internally. Nested diagrams, annotations, ELK,
etc. stay in ``xpict.future`` until they graduate here.
"""

from __future__ import annotations

from pydantic import BaseModel, ConfigDict, Field, model_validator


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class MolSpec(StrictModel):
    """One molecule entry in the preferred declarative document.

    Structure: pass exactly one of ``smiles``, ``cxsmiles``, ``molfile``, or
    ``source`` (alias for any structure string).

    Do **not** put list-index ``align_to`` here. Imperative alignment belongs
    on the simple client (``render(..., align_to=Mol|Rendered)``); document-
    level align references will grow with ``PictSpec``.
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
    """Preferred declarative document: ordered mols → ordered rendered results."""

    molecules: list[MolSpec] = Field(default_factory=list)
