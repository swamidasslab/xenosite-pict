"""IPython / notebook HTML — thin row/cell packer over Rust-painted SVGs.

Mol captions are painted in Rust into each mol scene. This module only lays out
cells/rows (flex). A future render option may move even this packing into Rust.
"""

from __future__ import annotations

from xpict.contracts.depict import (
    DepictSpecRoot,
    GridNode,
    GroupNode,
    MolNode,
    ReactionSchemeNode,
)
from xpict.contracts.depict import DepictSpec as WireDepictSpec
from xpict.depict_spec import as_depict_spec, depict
from xpict.rendered import Rendered


def _html_svg(svg: str) -> str:
    return f'<div style="display:inline-block;line-height:0">{svg}</div>'


def _html_row(svgs: list[str]) -> str:
    cells = "".join(_html_svg(s) for s in svgs)
    return (
        '<div style="display:flex;flex-wrap:wrap;gap:0.35rem;'
        f'align-items:flex-end">{cells}</div>'
    )


def _html_grid(rows_html: list[str]) -> str:
    body = "".join(f"<div>{row}</div>" for row in rows_html)
    return (
        '<div style="display:flex;flex-direction:column;gap:0.5rem;'
        f'align-items:flex-start">{body}</div>'
    )


def _pack_cells(cells: list[Rendered]) -> str:
    if not cells:
        return ""
    if len(cells) == 1:
        return _html_svg(cells[0].to_svg())
    return _html_row([c.to_svg() for c in cells])


def _cell_count(row: DepictSpecRoot | WireDepictSpec) -> int:
    """Cells Rust ``render`` emits for one grid row (schemes compose to one)."""
    root: DepictSpecRoot = row.root if isinstance(row, WireDepictSpec) else row
    if isinstance(root, ReactionSchemeNode):
        return 1
    if isinstance(root, MolNode):
        return 1
    if isinstance(root, GridNode):
        return sum(_cell_count(child) for child in root.children)
    if isinstance(root, GroupNode):
        return sum(1 for child in root.children if isinstance(child, MolNode))
    return 0


def render_depict_mols(spec: WireDepictSpec) -> list[Rendered]:
    """Product paint path (nested schemes → composed cells)."""
    return depict(as_depict_spec(spec))


def render_depict_html(spec: WireDepictSpec) -> str:
    """Paint the full spec once in Rust; pack cells into HTML rows."""
    doc = as_depict_spec(spec)
    cells = depict(doc)
    root = doc.root
    if isinstance(root, GridNode):
        rows: list[str] = []
        i = 0
        for row in root.children:
            n = _cell_count(row)
            rows.append(_pack_cells(cells[i : i + n]))
            i += n
        return _html_grid(rows)
    return _pack_cells(cells)
