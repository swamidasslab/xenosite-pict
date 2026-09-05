"""Layout backend protocol and registry."""

from __future__ import annotations

import warnings
from typing import Protocol, runtime_checkable

from xenosite.pict.contracts.layout import MoleculeLayout
from xenosite.pict.contracts.spec import MoleculeSpec
from xenosite.pict.warnings import PictBackendWarning


@runtime_checkable
class LayoutBackend(Protocol):
    name: str

    def layout(self, mol: MoleculeSpec) -> MoleculeLayout:
        """Return 2D coordinates + topology for one molecule."""
        ...


_REGISTRY: dict[str, type] = {}


def register(name: str):
    def deco(cls):
        _REGISTRY[name] = cls
        return cls

    return deco


def get_backend(name: str) -> LayoutBackend:
    key = name.lower()
    if key not in _REGISTRY:
        available = ", ".join(sorted(_REGISTRY)) or "(none registered)"
        raise KeyError(f"Unknown layout backend {name!r}. Available: {available}")
    return _REGISTRY[key]()


def warn_unsupported(backend: str, option: str, detail: str = "") -> None:
    msg = f"Backend {backend!r} does not support option {option!r}."
    if detail:
        msg = f"{msg} {detail}"
    warnings.warn(msg, PictBackendWarning, stacklevel=3)
