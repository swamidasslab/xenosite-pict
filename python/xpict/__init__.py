"""xpict — molecule depiction.

Two first-class APIs (same paint as JS / Rust):

**Single molecule** — ``mol`` / ``render`` / ``to_svg``::

    from xpict import mol, render, to_svg

    benzene = mol("c1ccccc1")
    rendered = render(benzene, {"color": "#0b6e4f"})
    svg = to_svg(rendered.scene)

**Declarative document** — nested ``DepictSpec`` via Rust two-pass ``depict``::

    from xpict import depict, to_svg

    rows = depict({"type": "mol", "smiles": "CCO"})
    svg = to_svg(rows[0].scene)
"""

from __future__ import annotations

from typing import Any, overload

from xpict.api import Pict
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
from xpict.depict_spec import depict, depict_svg, is_live_depict_spec
from xpict.warnings import PictBackendWarning

# Lab / future nested document.
from xpict.edge_plan import (
    build_align_plan,
    process_edge_plan,
    process_edge_plan_with_frames,
    validate_edge_plan,
)
from xpict.contracts.edge import EdgePlan, EdgeResult, MolTemplate
from xpict.future import PictSpec
from xpict.native_bridge import plan_edge, render_doc

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
    "depict_svg",
    "mol",
    "plan_edge",
    "process_edge_plan",
    "process_edge_plan_with_frames",
    "render",
    "render_doc",
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
    """Single-mol ``render(mol, opts)`` or document ``render(doc)``.

    Live nested ``DepictSpec`` (``type: mol|group``) uses the Rust two-pass
    (``depict`` → SVG). Legacy flat / future docs still go through ``Pict``.
    """
    if isinstance(input, (Mol, str)):
        return render_mol(input, opts)
    if opts is not None:
        raise TypeError(
            "document render() does not take a second positional opts; "
            "use backend=/format= keywords, or mol()/render(mol, opts) for single-mol"
        )
    if is_live_depict_spec(input):
        if format != "svg":
            raise ValueError("live DepictSpec render() only supports format='svg' today")
        return depict_svg(input)
    return Pict(backend=backend, format=format).render(input)
