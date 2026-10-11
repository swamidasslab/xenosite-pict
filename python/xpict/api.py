"""Public Pict / render API — native Rust ``depict`` only (no layout backends)."""

from __future__ import annotations

from typing import Any, Literal

from xpict.contracts.edge import MoleculeIn
from xpict.depict_spec import as_depict_spec, depict, depict_svg, is_live_depict_spec
from xpict.future.nodes import PictSpec, expand_pict
from xpict.future.spec import DiagramKind, LegacyPictSpec
from xpict.rendered import scene_to_html

OutputFormat = Literal["svg", "html"]

DepictInput = PictSpec | LegacyPictSpec | dict[str, Any]


def _to_legacy(spec: DepictInput) -> LegacyPictSpec:
    """Accept nested PictSpec, legacy flat doc, or JSON dict → flat render doc."""
    if isinstance(spec, LegacyPictSpec):
        return spec
    tree = expand_pict(spec)
    return tree.to_legacy()


def _mol_node_from_legacy(m: Any) -> dict[str, Any]:
    node: dict[str, Any] = {"type": "mol"}
    if m.id:
        node["id"] = m.id
    if m.smiles:
        node["smiles"] = m.smiles
    if getattr(m, "cxsmiles", None):
        node["cxsmiles"] = m.cxsmiles
    if getattr(m, "molfile", None):
        node["molfile"] = m.molfile
    if getattr(m, "weight", None) is not None:
        node["weight"] = m.weight
    if getattr(m, "color", None) is not None:
        node["color"] = m.color
    if getattr(m, "scale", None) is not None:
        node["scale"] = m.scale
    if getattr(m, "halo", None) is not None:
        node["halo"] = m.halo
    shade = getattr(m, "shade", None)
    if shade is not None:
        if hasattr(shade, "model_dump"):
            node["shade"] = shade.model_dump(exclude_none=True)
        else:
            node["shade"] = shade
    star = getattr(m, "star_labels", None)
    if star is not None:
        node["star_labels"] = star
    title = getattr(m, "title", None)
    if title:
        # Live captions are text-node refs; drop free-text title on the mol leaf.
        pass
    return node


def _legacy_scheme_to_live(doc: LegacyPictSpec) -> dict[str, Any]:
    """Map legacy ``diagram.kind=reaction|network`` docs onto live ``reaction_scheme``."""
    children: list[dict[str, Any]] = [_mol_node_from_legacy(m) for m in doc.molecules]

    for i, e in enumerate(doc.diagram.edges):
        label_ref = None
        if e.label and str(e.label).strip():
            tid = f"_edge_lab_{i}"
            children.append({"type": "text", "id": tid, "text": str(e.label).strip()})
            label_ref = {"id": tid, "pos": getattr(e, "label_pos", None) or "above"}
        edge: dict[str, Any] = {
            "type": "edge",
            "sources": list(e.sources),
            "targets": list(e.targets),
            "arrow": e.arrow.value if hasattr(e.arrow, "value") else str(e.arrow),
            "dashed": bool(e.dashed),
        }
        if e.color is not None:
            edge["color"] = e.color
        if e.stroke_width is not None:
            edge["stroke_width"] = e.stroke_width
        if e.role is not None:
            edge["role"] = e.role
        if e.edge_routing is not None:
            edge["edge_routing"] = e.edge_routing
        if label_ref is not None:
            edge["label"] = label_ref
        children.append(edge)

    layout: dict[str, Any] = {}
    d = doc.diagram
    if d.algorithm is not None:
        layout["algorithm"] = d.algorithm
    if d.edge_routing is not None:
        layout["edge_routing"] = d.edge_routing
    if d.node_spacing is not None:
        layout["node_spacing"] = d.node_spacing
    if d.layer_spacing is not None:
        layout["layer_spacing"] = d.layer_spacing

    out: dict[str, Any] = {"type": "reaction_scheme", "children": children}
    if layout:
        out["layout"] = layout
    return out


def _legacy_flat_to_live(doc: LegacyPictSpec) -> dict[str, Any]:
    """Map legacy molecule / grid docs onto live ``mol`` / ``grid``."""
    mols = [_mol_node_from_legacy(m) for m in doc.molecules]
    if not mols:
        raise ValueError("legacy PictSpec has no molecules")
    columns = getattr(doc.diagram, "columns", None) or 0
    if len(mols) == 1 and not columns:
        return mols[0]
    cols = columns if columns and columns > 0 else len(mols)
    rows: list[dict[str, Any]] = []
    for i in range(0, len(mols), cols):
        chunk = mols[i : i + cols]
        if len(chunk) == 1:
            rows.append(chunk[0])
        else:
            rows.append({"type": "group", "align": False, "children": chunk})
    return {"type": "grid", "children": rows}


def to_live_depict(spec: DepictInput) -> dict[str, Any]:
    """Normalize any accepted document into a live DepictSpec dict."""
    if isinstance(spec, dict) and is_live_depict_spec(spec):
        return spec
    if is_live_depict_spec(spec):  # type: ignore[arg-type]
        return as_depict_spec(spec).model_dump(mode="json")  # type: ignore[arg-type]
    doc = _to_legacy(spec)
    if doc.diagram.kind in {DiagramKind.reaction, DiagramKind.network}:
        return _legacy_scheme_to_live(doc)
    return _legacy_flat_to_live(doc)


class Pict:
    """Native Rust depiction engine (no layout-backend switch)."""

    def __init__(self, *, format: OutputFormat = "svg") -> None:
        self.format: OutputFormat = format

    def render(
        self,
        spec: DepictInput,
        *,
        format: OutputFormat | None = None,
    ) -> str:
        live = to_live_depict(spec)
        out = format or self.format
        if out == "svg":
            return depict_svg(live)
        if out == "html":
            rows = depict(live)
            if not rows:
                raise RuntimeError("depict returned no molecules")
            return scene_to_html(rows[0].scene)
        raise ValueError(f"unsupported format: {out!r}")

    def layout(self, spec: DepictInput) -> list[MoleculeIn]:
        """Lay out molecules via Rust ``depict`` (``MoleculeIn`` from painted rows)."""
        return [r.molecule for r in depict(to_live_depict(spec))]


def render(
    spec: DepictInput,
    *,
    format: OutputFormat = "svg",
) -> str:
    """Shorthand for ``Pict(format=...).render(spec)``."""
    return Pict(format=format).render(spec)
