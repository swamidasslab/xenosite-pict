"""Public Pict / render API."""

from __future__ import annotations

from typing import Any, Literal

from xpict.align import align_layouts
from xpict.backends import get_backend
from xpict.contracts.layout import MoleculeLayout
from xpict.depict_spec import depict_svg, is_live_depict_spec
from xpict.future.nodes import PictSpec, expand_pict
from xpict.future.spec import DiagramKind, LegacyPictSpec
from xpict.draw.scene_builder import build_scene
from xpict.draw.svg import scene_to_html, scene_to_svg

OutputFormat = Literal["svg", "html"]


def _resolve_backend_name(requested: str | None) -> str:
    """Default: RDKit when installed. Indigo only if explicitly requested."""
    if requested:
        key = requested.lower()
        if key == "native":
            raise ValueError(
                "layout backend 'native' was removed; use 'rdkit' (default) or 'indigo'"
            )
        return key
    try:
        import importlib.util

        if importlib.util.find_spec("rdkit") is not None:
            return "rdkit"
    except (ImportError, ValueError, ModuleNotFoundError):
        pass
    raise ImportError(
        "No layout backend available. Install RDKit: pip install 'xpict[rdkit]' "
        "(or pass backend='indigo' with xpict[indigo])."
    )


def _to_legacy(spec: PictSpec | LegacyPictSpec | dict[str, Any]) -> LegacyPictSpec:
    """Accept nested PictSpec, legacy flat doc, or JSON dict → flat render doc."""
    if isinstance(spec, LegacyPictSpec):
        return spec
    tree = expand_pict(spec)
    return tree.to_legacy()


def _legacy_scheme_to_live(doc: LegacyPictSpec) -> dict[str, Any]:
    """Map legacy ``diagram.kind=reaction|network`` docs onto live ``reaction_scheme``."""
    children: list[dict[str, Any]] = []
    for m in doc.molecules:
        node: dict[str, Any] = {"type": "mol"}
        if m.id:
            node["id"] = m.id
        if m.smiles:
            node["smiles"] = m.smiles
        if getattr(m, "cxsmiles", None):
            node["cxsmiles"] = m.cxsmiles
        if getattr(m, "molfile", None):
            node["molfile"] = m.molfile
        children.append(node)

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


class Pict:
    """Configured depiction engine.

    Backend and output options are runtime config — not part of PictSpec JSON.
    Reaction / network diagrams render via the live Rust ``reaction_scheme`` path.
    """

    def __init__(
        self,
        backend: str | None = None,
        *,
        format: OutputFormat = "svg",
    ) -> None:
        self.backend = _resolve_backend_name(backend)
        self.format: OutputFormat = format

    def render(
        self,
        spec: PictSpec | LegacyPictSpec | dict[str, Any],
        *,
        format: OutputFormat | None = None,
    ) -> str:
        if is_live_depict_spec(spec):
            out = format or self.format
            if out != "svg":
                raise ValueError("live DepictSpec render() only supports format='svg'")
            return depict_svg(spec)  # type: ignore[arg-type]

        doc = _to_legacy(spec)
        out = format or self.format
        if doc.diagram.kind in {DiagramKind.reaction, DiagramKind.network}:
            if out != "svg":
                raise ValueError("reaction_scheme render() only supports format='svg'")
            return depict_svg(_legacy_scheme_to_live(doc))

        backend = get_backend(self.backend)
        layouts: list[MoleculeLayout] = [backend.layout(m) for m in doc.molecules]
        layouts = align_layouts(layouts, enabled=doc.diagram.align, specs=doc.molecules)
        # Row / grid only — scheme compose lives in Rust ``compose_scheme``.
        positions = None
        if doc.diagram.kind == DiagramKind.grid:
            from xpict.draw.scene_builder import grid_positions, viewport_size
            from xpict.draw.metrics import shared_coord_scale

            scale = shared_coord_scale(layouts)
            sizes = [
                viewport_size(
                    layout,
                    doc.molecules[i] if i < len(doc.molecules) else None,
                    scale=scale,
                )
                for i, layout in enumerate(layouts)
            ]
            cols = doc.diagram.columns or max(1, int(len(layouts) ** 0.5 + 0.5))
            positions = grid_positions(layouts, cols, sizes)

        scene = build_scene(
            layouts,
            doc.molecules,
            doc,
            positions=positions,
        )
        if out == "html":
            return scene_to_html(scene)
        return scene_to_svg(scene)

    def layout(
        self, spec: PictSpec | LegacyPictSpec | dict[str, Any]
    ) -> list[MoleculeLayout]:
        """Lay out molecules for the legacy Pict/draw path (internal helper)."""
        doc = _to_legacy(spec)
        backend = get_backend(self.backend)
        layouts = align_layouts(
            [backend.layout(m) for m in doc.molecules],
            enabled=doc.diagram.align,
            specs=doc.molecules,
        )
        return layouts


def render(
    spec: PictSpec | LegacyPictSpec | dict[str, Any],
    *,
    backend: str | None = None,
    format: OutputFormat = "svg",
) -> str:
    """Shorthand for ``Pict(backend=...).render(spec)``."""
    return Pict(backend=backend, format=format).render(spec)
