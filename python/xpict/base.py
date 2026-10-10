"""Shared host behaviors: ``to_spec``, ``opts``, operators."""

from __future__ import annotations

from typing import Unpack

from pydantic import TypeAdapter

from xpict.contracts.depict import (
    CommonOpts,
    MolNode,
    MolOptsPatch,
    Opts,
    OptsPatch,
    ShadeStyle,
)
from xpict.contracts.depict import DepictSpec as WireDepictSpec
from xpict.contracts.depict_dict import (
    CommonOptsDict,
    DepictSpecDict,
    OptsPatchDict,
    ShadeStyleDict,
)
from xpict.sugar import Operand, group_add, grid_join, reaction_join

_OPTS_ADAPTER: TypeAdapter[Opts] = TypeAdapter(Opts)

_MOL_ONLY_OPT_KEYS = frozenset({"weight", "halo", "shade"})


class OptsKeywords(CommonOptsDict, total=False):
    """Keyword form of ``opts()`` — ``CommonOpts`` plus mol-only keys."""

    weight: float
    halo: bool
    shade: ShadeStyle | ShadeStyleDict


def _as_opt_patches(*patches: Opts | OptsPatchDict) -> list[OptsPatch]:
    out: list[OptsPatch] = []
    for raw in patches:
        if isinstance(raw, dict) and raw.get("type") == "grid":
            raw = {**raw, "type": "group"}
        validated = _OPTS_ADAPTER.validate_python(raw)
        if isinstance(validated, list):
            out.extend(validated)
        else:
            out.append(validated)
    return out


def _append_opts(existing: Opts | None, patches: list[OptsPatch]) -> Opts | None:
    if not patches:
        return existing
    if existing is None:
        bag = list(patches)
    elif isinstance(existing, list):
        bag = [*existing, *patches]
    else:
        bag = [existing, *patches]
    return bag[0] if len(bag) == 1 else bag


def apply_opts(
    spec: WireDepictSpec | DepictSpecDict,
    *patches: Opts | OptsPatchDict,
    **kwargs: Unpack[OptsKeywords],
) -> WireDepictSpec:
    """Apply paint/cascade opts; returns wire RootModel (caller may re-wrap)."""
    unknown = set(kwargs) - frozenset(CommonOpts.model_fields) - _MOL_ONLY_OPT_KEYS
    if unknown:
        raise TypeError(
            f"opts() got unexpected keyword(s): {sorted(unknown)}; "
            f"allowed: {sorted(frozenset(CommonOpts.model_fields) | _MOL_ONLY_OPT_KEYS)}"
        )

    doc = spec if isinstance(spec, WireDepictSpec) else WireDepictSpec.model_validate(spec)
    root = doc.root
    root_fields = set(type(root).model_fields)
    field_updates = {
        key: kwargs[key]
        for key in CommonOpts.model_fields
        if kwargs.get(key) is not None and key in root_fields
    }

    extra_patches = _as_opt_patches(*patches)
    weight = kwargs.get("weight")
    halo = kwargs.get("halo")
    shade = kwargs.get("shade")
    if isinstance(root, MolNode):
        if weight is not None:
            field_updates["weight"] = weight
        if halo is not None:
            field_updates["halo"] = halo
        if shade is not None:
            extra_patches.append(
                MolOptsPatch(type="mol", shade=ShadeStyle.model_validate(shade))
            )
    elif weight is not None or halo is not None or shade is not None:
        mol_patch = MolOptsPatch(type="mol")
        if weight is not None:
            mol_patch = mol_patch.model_copy(update={"weight": weight})
        if halo is not None:
            mol_patch = mol_patch.model_copy(update={"halo": halo})
        if shade is not None:
            mol_patch = mol_patch.model_copy(
                update={"shade": ShadeStyle.model_validate(shade)}
            )
        extra_patches.append(mol_patch)

    common = {k: kwargs[k] for k in CommonOpts.model_fields if kwargs.get(k) is not None}
    if common:
        extra_patches.append(CommonOpts.model_validate(common))

    merged = _append_opts(getattr(root, "opts", None), extra_patches)
    if merged is not None:
        field_updates["opts"] = merged

    if not field_updates:
        return doc
    return type(doc)(root.model_copy(update=field_updates))


def _wrap_wire(left: WireDepictSpec, wire: WireDepictSpec) -> WireDepictSpec:
    """Re-wrap sugar output with the same RootModel subclass as ``left``."""
    return type(left)(wire.root)


class PictBase:
    """Common host API: ``to_spec()``, ``opts()``, document operators."""

    def to_spec(self) -> WireDepictSpec:
        raise NotImplementedError

    def opts(
        self, *patches: Opts | OptsPatchDict, **kwargs: Unpack[OptsKeywords]
    ) -> WireDepictSpec:
        left = self.to_spec()
        return _wrap_wire(left, apply_opts(left, *patches, **kwargs))

    def __add__(self, other: Operand) -> WireDepictSpec:
        left = self.to_spec()
        return _wrap_wire(left, group_add(left, other))

    def __radd__(self, other: Operand) -> WireDepictSpec:
        right = self.to_spec()
        return _wrap_wire(right, group_add(other, right))

    def __rshift__(self, other: Operand) -> WireDepictSpec:
        left = self.to_spec()
        return _wrap_wire(left, reaction_join(left, other, reverse=False))

    def __rrshift__(self, other: Operand) -> WireDepictSpec:
        right = self.to_spec()
        return _wrap_wire(right, reaction_join(other, right, reverse=False))

    def __lshift__(self, other: Operand) -> WireDepictSpec:
        left = self.to_spec()
        return _wrap_wire(left, reaction_join(left, other, reverse=True))

    def __rlshift__(self, other: Operand) -> WireDepictSpec:
        right = self.to_spec()
        return _wrap_wire(right, reaction_join(other, right, reverse=True))

    def __or__(self, other: Operand) -> WireDepictSpec:
        left = self.to_spec()
        return _wrap_wire(left, grid_join(left, other))

    def __ror__(self, other: Operand) -> WireDepictSpec:
        right = self.to_spec()
        return _wrap_wire(right, grid_join(other, right))
