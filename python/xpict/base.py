"""Shared host behaviors: ``to_spec``, ``opts``, operators."""

from __future__ import annotations

from typing import Unpack

from pydantic import TypeAdapter

from xpict.contracts.depict import MolOpts, Opts, OptsPatch
from xpict.contracts.depict import DepictSpec as WireDepictSpec
from xpict.contracts.depict_dict import MolOptsDict, OptsPatchDict
from xpict.sugar import Operand, group_add, grid_join, reaction_join

_OPTS_ADAPTER: TypeAdapter[Opts] = TypeAdapter(Opts)

# Keyword form of ``opts()`` — universal cascade bag ([`MolOpts`], incl. weight/halo).
OptsKeywords = MolOptsDict


class PictBase:
    """Common host API: ``to_spec()``, ``opts()``, document operators, notebook repr."""

    def to_spec(self) -> WireDepictSpec:
        raise NotImplementedError

    def opts(
        self, *patches: Opts | OptsPatchDict, **kwargs: Unpack[OptsKeywords]
    ) -> WireDepictSpec:
        """Shallow-copy the root and append cascade opts (kwargs → ``MolOpts``)."""
        left = self.to_spec()
        root = left.root
        extra: list[OptsPatch] = []
        for raw in patches:
            validated = _OPTS_ADAPTER.validate_python(raw)
            if isinstance(validated, list):
                extra.extend(validated)
            else:
                extra.append(validated)
        common = {k: v for k, v in kwargs.items() if v is not None}
        if common:
            extra.append(MolOpts.model_validate(common))
        if not extra:
            return left
        existing = getattr(root, "opts", None)
        if existing is None:
            merged: Opts = extra[0] if len(extra) == 1 else extra
        elif isinstance(existing, list):
            merged = [*existing, *extra]
        else:
            merged = [existing, *extra]
        return type(left)(root.model_copy(update={"opts": merged}))

    def __add__(self, other: Operand) -> WireDepictSpec:
        left = self.to_spec()
        return type(left)(group_add(left, other).root)

    def __radd__(self, other: Operand) -> WireDepictSpec:
        right = self.to_spec()
        return type(right)(group_add(other, right).root)

    def __rshift__(self, other: Operand) -> WireDepictSpec:
        left = self.to_spec()
        return type(left)(reaction_join(left, other, reverse=False).root)

    def __rrshift__(self, other: Operand) -> WireDepictSpec:
        right = self.to_spec()
        return type(right)(reaction_join(other, right, reverse=False).root)

    def __lshift__(self, other: Operand) -> WireDepictSpec:
        left = self.to_spec()
        return type(left)(reaction_join(left, other, reverse=True).root)

    def __rlshift__(self, other: Operand) -> WireDepictSpec:
        right = self.to_spec()
        return type(right)(reaction_join(other, right, reverse=True).root)

    def __or__(self, other: Operand) -> WireDepictSpec:
        left = self.to_spec()
        return type(left)(grid_join(left, other).root)

    def __ror__(self, other: Operand) -> WireDepictSpec:
        right = self.to_spec()
        return type(right)(grid_join(other, right).root)

    def _repr_html_(self) -> str:
        from xpict.display import render_depict_html

        return render_depict_html(self.to_spec())

    def _repr_svg_(self) -> str | None:
        from xpict.display import render_depict_mols

        rows = render_depict_mols(self.to_spec())
        if len(rows) == 1:
            return rows[0].to_svg()
        return None

    def _repr_mimebundle_(
        self,
        include: list[str] | None = None,
        exclude: list[str] | None = None,
    ) -> dict[str, str]:
        _ = include, exclude
        spec = self.to_spec()
        from xpict.sugar import mols_of

        return {
            "text/html": self._repr_html_(),
            "text/plain": f"{type(self).__name__}({spec.root.type}, {len(mols_of(spec))} mols)",
        }
