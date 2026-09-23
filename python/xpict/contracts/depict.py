# Auto-generated from Rust schemars (make types) — do not edit.
"""DepictSpec — live document ABI from xpict-core (schemars)."""

from __future__ import annotations

from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field, RootModel


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")

    def model_dump(self, *args, **kwargs):
        kwargs.setdefault("exclude_none", True)
        return super().model_dump(*args, **kwargs)

    def model_dump_json(self, *args, **kwargs):
        kwargs.setdefault("exclude_none", True)
        return super().model_dump_json(*args, **kwargs)

class AlignToSpec(StrictModel):
    """Object form of document ``align_to`` (template ref + EdgePlan-style opts)."""
    ref: Annotated[str, Field(description='Id of the template mol in this group.')]
    atom_map: Annotated[list[tuple[int, int]] | None, Field(description='Pairs `(query, template)` vs the template; skips MCS when set.')] = None
    min_atoms: Annotated[int | None, Field(description='Override [`crate::edge::MIN_MCS_ATOMS`] when set.')] = None


AlignTo = str | AlignToSpec


class ShadeSpec(StrictModel):
    """Per-atom / per-bond colormap scores."""
    atoms: list[float] | None = None
    bonds: list[float] | None = None
    colormap: str | None = None
    vmax: float = 1.0
    vmin: float = 0.0


MolNodeKind = Literal['mol']


class MolNode(StrictModel):
    """Mol node — subset of future ``MolNode``."""
    type: Literal['mol'] = 'mol'
    align_to: Annotated[AlignTo | None, Field(description='Template id string, or `{ "ref", "atom_map"?, "min_atoms"? }`.')] = None
    color: str | None = None
    cxsmiles: str | None = None
    id: str | None = None
    molfile: str | None = None
    scale: float | None = None
    shade: ShadeSpec | None = None
    smiles: str | None = None
    star_labels: list[str | None] | None = None
    weight: float | None = None


class GroupNode(StrictModel):
    type: Literal['group'] = 'group'
    align: bool = Field(default=False, description='When true, later children align onto the first (or each `align_to`).')
    children: list[MolNode] = Field(default_factory=list)
    id: str | None = None


DepictSpecRoot = Annotated[MolNode | GroupNode, Field(discriminator='type')]

class DepictSpec(RootModel[DepictSpecRoot]):
    """Declarative document (`mol` or `group` root)."""

    def model_dump(self, *args, **kwargs):
        kwargs.setdefault('exclude_none', True)
        return super().model_dump(*args, **kwargs)

    def model_dump_json(self, *args, **kwargs):
        kwargs.setdefault('exclude_none', True)
        return super().model_dump_json(*args, **kwargs)

    def mols(self) -> list[MolNode]:
        """Flatten mol root or group children (host helper, not on the wire)."""
        root = self.root
        if isinstance(root, MolNode):
            return [root]
        return list(root.children)



MolSpec = MolNode
