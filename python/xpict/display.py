"""IPython / notebook HTML for DepictSpec (layout only; paint from Rust)."""

from __future__ import annotations

from xpict.contracts.depict import (
    EdgeNode,
    GridNode,
    GroupNode,
    MolNode,
    ReactionSchemeNode,
    TextNode,
)
from xpict.contracts.depict import DepictSpec as WireDepictSpec
from xpict.depict_spec import as_depict_spec, as_rendered
from xpict.native_bridge import depict as _native_depict
from xpict.rendered import Rendered
from xpict.sugar import mols_of


def _html_svg(svg: str) -> str:
    return f'<div style="display:inline-block;line-height:0">{svg}</div>'


def _html_row(svgs: list[str]) -> str:
    cells = "".join(_html_svg(s) for s in svgs)
    return (
        '<div style="display:flex;flex-wrap:wrap;gap:1rem;'
        f'align-items:center">{cells}</div>'
    )


def _html_grid(rows_html: list[str]) -> str:
    body = "".join(
        f'<div style="display:flex;flex-wrap:wrap;gap:0.5rem;'
        f'align-items:center">{row}</div>'
        for row in rows_html
    )
    return (
        '<div style="display:flex;flex-direction:column;gap:1rem;'
        f'align-items:flex-start">{body}</div>'
    )


def _mol_id(node: MolNode, i: int) -> str:
    return (node.id or "").strip() or f"m_{i}"


def render_depict_mols(spec: WireDepictSpec) -> list[Rendered]:
    return [as_rendered(row) for row in _native_depict(as_depict_spec(spec))]


def _edge_arrow_html(edge: EdgeNode, children: list[MolNode | EdgeNode | TextNode]) -> str:
    index = {
        str(c.id): i
        for i, c in enumerate(children)
        if isinstance(c, MolNode) and c.id is not None
    }

    def _ids(raw: str | list[str]) -> list[str]:
        if isinstance(raw, list):
            return [str(x) for x in raw]
        return [str(raw)]

    src_i = min((index[s] for s in _ids(edge.sources) if s in index), default=0)
    tgt_i = min((index[t] for t in _ids(edge.targets) if t in index), default=1)
    glyph = "←" if tgt_i < src_i else "→"
    if edge.arrow == "equilibrium":
        glyph = "⇌"
    return (
        f'<span style="font-size:1.75rem;line-height:1;padding:0 0.25rem">{glyph}</span>'
    )


def _row_wire(
    row: MolNode | GroupNode | ReactionSchemeNode | GridNode | WireDepictSpec,
) -> WireDepictSpec:
    if isinstance(row, WireDepictSpec):
        return row
    return WireDepictSpec(row)


def _render_row_html(
    row_spec: WireDepictSpec, painted_by_id: dict[str, Rendered]
) -> str:
    root = row_spec.root
    if isinstance(root, ReactionSchemeNode):
        parts: list[str] = []
        for child in root.children:
            if isinstance(child, MolNode):
                r = painted_by_id.get((child.id or "").strip())
                if r is not None:
                    parts.append(_html_svg(r.to_svg()))
            elif isinstance(child, EdgeNode):
                parts.append(_edge_arrow_html(child, list(root.children)))
        return "".join(parts)

    return "".join(
        _html_svg(painted_by_id[_mol_id(n, i)].to_svg())
        for i, n in enumerate(mols_of(row_spec))
        if _mol_id(n, i) in painted_by_id
    )


def render_depict_html(spec: WireDepictSpec) -> str:
    painted = render_depict_mols(spec)
    painted_by_id = {
        _mol_id(n, i): r for i, (n, r) in enumerate(zip(mols_of(spec), painted))
    }
    root = spec.root
    if isinstance(root, GridNode):
        row_html = [
            _render_row_html(_row_wire(row), painted_by_id) for row in root.children
        ]
        return _html_grid(row_html)
    if isinstance(root, ReactionSchemeNode):
        inner = _render_row_html(spec, painted_by_id)
        return (
            '<div style="display:flex;flex-wrap:wrap;gap:0.5rem;'
            f'align-items:center">{inner}</div>'
        )
    if not painted:
        return ""
    if len(painted) == 1:
        return _html_svg(painted[0].to_svg())
    return _html_row([r.to_svg() for r in painted])
