"""xpict — molecule depiction.

Two first-class APIs (same paint as JS / Rust):

**Single molecule** — ``mol`` / ``render`` / ``to_svg``::

    from xpict import mol, render, to_svg

    benzene = mol("c1ccccc1")
    rendered = render(benzene, MolRenderOptions(color="#0b6e4f"))
    svg = to_svg(rendered.scene)

**Declarative document** — nested ``DepictSpec`` via Rust ``depict``::

    from xpict import depict, to_svg

    rows = depict({"type": "mol", "smiles": "CCO"})
    svg = to_svg(rows[0].scene)
"""

from __future__ import annotations

from typing import overload

from xpict.api import OutputFormat, Pict
from xpict.base import PictBase
from xpict.contracts import DepictSpecDict, MolSpec
from xpict.contracts.edge import EdgePlan, EdgeResult, MolTemplate
from xpict.contracts.scene import Scene
from xpict.depict_spec import depict, depict_svg, is_live_depict_spec
from xpict.future import PictSpec
from xpict.native_bridge import plan_edge, render_doc, validate_edge_plan
from xpict.rendered import Rendered, SvgAtom, SvgBond, to_svg
from xpict.spec import DepictSpec, Mol, MolRenderOptions, mol, render_mol

__version__ = "0.4.0"

__all__ = [
    "DepictSpec",
    "DepictSpecDict",
    "EdgePlan",
    "EdgeResult",
    "Mol",
    "MolRenderOptions",
    "MolSpec",
    "MolTemplate",
    "Pict",
    "PictBase",
    "PictSpec",  # future nested document — use DepictSpec for the shipped subset
    "Rendered",
    "Scene",
    "SvgAtom",
    "SvgBond",
    "depict",
    "depict_svg",
    "mol",
    "plan_edge",
    "render",
    "render_doc",
    "to_svg",
    "validate_edge_plan",
    "__version__",
]


@overload
def render(
    input: Mol | str,
    opts: MolRenderOptions | None = None,
) -> Rendered: ...


@overload
def render(
    input: DepictSpec | DepictSpecDict | PictSpec,
    opts: None = None,
    *,
    format: OutputFormat = "svg",
) -> str: ...


def render(
    input: Mol | str | DepictSpec | DepictSpecDict | PictSpec,
    opts: MolRenderOptions | None = None,
    *,
    format: OutputFormat = "svg",
) -> Rendered | str:
    """Single-mol ``render(mol, opts)`` or document ``render(doc)`` via Rust."""
    if isinstance(input, (Mol, str)):
        return render_mol(input, opts)
    if opts is not None:
        raise TypeError(
            "document render() does not take a second positional opts; "
            "use format= keywords, or mol()/render(mol, opts) for single-mol"
        )
    if isinstance(input, DepictSpec) or (
        isinstance(input, dict) and is_live_depict_spec(input)
    ):
        if format != "svg":
            return Pict(format=format).render(input)
        return depict_svg(input)
    return Pict(format=format).render(input)
