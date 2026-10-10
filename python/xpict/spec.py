"""Host DepictSpec — wire RootModel + PictBase + operator sugar."""

from __future__ import annotations

from xpict.base import PictBase
from xpict.contracts.depict import (
    GridNode,
    GroupNode,
    MolNode,
    ReactionSchemeNode,
)
from xpict.contracts.depict import DepictSpec as WireDepictSpec
from xpict.display import render_depict_html, render_depict_mols
from xpict.sugar import (
    Operand,
    group_add,
    grid_join,
    mols_of,
    reaction_join,
)


class DepictSpec(PictBase, WireDepictSpec):
    """Live document root — generated wire model + host operators."""

    def to_spec(self) -> DepictSpec:
        return self

    def mols(self) -> list[MolNode]:
        return mols_of(self)

    def __add__(self, other: Operand) -> DepictSpec:
        return DepictSpec(group_add(self, other).root)

    def __radd__(self, other: Operand) -> DepictSpec:
        return DepictSpec(group_add(other, self).root)

    def __rshift__(self, other: Operand) -> DepictSpec:
        return DepictSpec(reaction_join(self, other, reverse=False).root)

    def __rrshift__(self, other: Operand) -> DepictSpec:
        return DepictSpec(reaction_join(other, self, reverse=False).root)

    def __lshift__(self, other: Operand) -> DepictSpec:
        return DepictSpec(reaction_join(self, other, reverse=True).root)

    def __rlshift__(self, other: Operand) -> DepictSpec:
        return DepictSpec(reaction_join(other, self, reverse=True).root)

    def __or__(self, other: Operand) -> DepictSpec:
        return DepictSpec(grid_join(self, other).root)

    def __ror__(self, other: Operand) -> DepictSpec:
        return DepictSpec(grid_join(other, self).root)

    def _repr_html_(self) -> str:
        return render_depict_html(self)

    def _repr_svg_(self) -> str | None:
        rows = render_depict_mols(self)
        if len(rows) == 1:
            return rows[0].to_svg()
        return None

    def _repr_mimebundle_(
        self,
        include: list[str] | None = None,
        exclude: list[str] | None = None,
    ) -> dict[str, str]:
        _ = include, exclude
        return {
            "text/html": self._repr_html_(),
            "text/plain": f"DepictSpec({self.root.type}, {len(self.mols())} mols)",
        }


def row_as_spec(
    row: MolNode | GroupNode | ReactionSchemeNode | GridNode | WireDepictSpec,
) -> DepictSpec:
    """Host DepictSpec for a grid row root."""
    if isinstance(row, DepictSpec):
        return row
    if isinstance(row, WireDepictSpec):
        return DepictSpec(row.root)
    return DepictSpec(row)
