"""xpict — molecule depiction.

Two first-class APIs (same paint as JS / Rust):

**Single molecule** — ``mol`` / ``render`` / ``to_svg``::

    from xpict import mol, render, to_svg

    benzene = mol("c1ccccc1")
    rendered = render(benzene, {"color": "#0b6e4f"})
    svg = to_svg(rendered.scene)

**Declarative document** — nested ``DepictSpec`` via ``depict`` / ``render(doc)``::

    from xpict import depict

    svg = depict({"type": "mol", "smiles": "CCO"})
"""

from __future__ import annotations

from typing import Any, overload

from xpict.api import Pict, render as depict
from xpict.client import (
    Mol,
    MolRenderOptions,
    Rendered,
    SvgAtom,
    SvgBond,
    mol,
    render as render_mol,
    to_svg,
)
from xpict.contracts.depict import DepictSpec, MolSpec
from xpict.contracts.scene import Scene
from xpict.warnings import PictBackendWarning

# Lab / future nested document.
from xpict.edge_plan import build_align_plan, process_edge_plan, validate_edge_plan
from xpict.contracts.edge import EdgePlan, EdgeResult, MolTemplate
from xpict.future import PictSpec

__version__ = "0.3.0"

__all__ = [
    "DepictSpec",
    "EdgePlan",
    "EdgeResult",
    "Mol",
    "MolRenderOptions",
    "MolSpec",
    "MolTemplate",
    "Pict",
    "PictBackendWarning",
    "PictSpec",  # future nested document — use DepictSpec for the shipped subset
    "Rendered",
    "Scene",
    "SvgAtom",
    "SvgBond",
    "build_align_plan",
    "depict",
    "mol",
    "process_edge_plan",
    "render",
    "to_svg",
    "validate_edge_plan",
    "__version__",
]


@overload
def render(input: Mol | str, opts: MolRenderOptions | dict[str, Any] | None = None) -> Rendered: ...


@overload
def render(
    input: DepictSpec | PictSpec | dict[str, Any],
    opts: None = None,
    *,
    backend: str | None = None,
    format: str = "svg",
) -> str: ...


def render(input: Any, opts: Any = None, *, backend: str | None = None, format: str = "svg") -> Any:
    """Single-mol ``render(mol, opts)`` or document ``render(doc)`` / ``depict(doc)``.

    Dispatches on the first argument: ``Mol`` / SMILES string → single-molecule
    client (parity with JS/Rust); nested ``DepictSpec`` dict → document SVG.
    """
    if isinstance(input, (Mol, str)):
        return render_mol(input, opts)
    if opts is not None:
        raise TypeError(
            "document render() does not take a second positional opts; "
            "use backend=/format= keywords, or mol()/render(mol, opts) for single-mol"
        )
    return depict(input, backend=backend, format=format)  # type: ignore[call-arg]
