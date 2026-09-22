"""Public Pict / render API — molecule ink via Rust ``depict_molecule`` (JS surface)."""

from __future__ import annotations

from typing import Any, Literal

from xpict.align import align_layouts
from xpict.backends import BACKEND_PREFERENCE, get_backend
from xpict.contracts.layout import LayoutResult, MoleculeLayout
from xpict.contracts.nodes import PictSpec, expand_pict
from xpict.contracts.spec import LegacyPictSpec
from xpict.diagram.elk import layout_diagram_ex
from xpict.draw.scene_builder import build_scene
from xpict.draw.svg import scene_to_html, scene_to_svg

OutputFormat = Literal["svg", "html"]


def _resolve_backend_name(requested: str | None) -> str:
    """Default: RDKit when installed, else native. Indigo only if requested."""
    if requested:
        return requested.lower()
    for name in BACKEND_PREFERENCE:
        if name == "indigo":
            continue
        if name == "native":
            return "native"
        if name == "rdkit":
            try:
                import rdkit  # noqa: F401

                return "rdkit"
            except ImportError:
                continue
    return "native"


def _to_legacy(spec: PictSpec | LegacyPictSpec | dict[str, Any]) -> LegacyPictSpec:
    if isinstance(spec, LegacyPictSpec):
        return spec
    tree = expand_pict(spec)
    return tree.to_legacy()


class Pict:
    """Configured depiction engine.

    Backend picks **layout** only; paint is always Rust ``depict_molecule``
    (same ABI as JS wasm).
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
        doc = _to_legacy(spec)
        backend = get_backend(self.backend)
        layouts: list[MoleculeLayout] = [backend.layout(m) for m in doc.molecules]
        layouts = align_layouts(layouts, enabled=doc.diagram.align, specs=doc.molecules)
        placement = layout_diagram_ex(layouts, doc)
        scene = build_scene(
            layouts,
            doc.molecules,
            doc,
            positions=placement.positions,
            edge_paths=placement.edge_paths,
            diagram_width=placement.width,
            diagram_height=placement.height,
        )
        out = format or self.format
        if out == "html":
            return scene_to_html(scene)
        return scene_to_svg(scene)

    def layout(
        self, spec: PictSpec | LegacyPictSpec | dict[str, Any]
    ) -> LayoutResult:
        doc = _to_legacy(spec)
        backend = get_backend(self.backend)
        layouts = align_layouts(
            [backend.layout(m) for m in doc.molecules],
            enabled=doc.diagram.align,
            specs=doc.molecules,
        )
        return LayoutResult(molecules=layouts)


def render(
    spec: PictSpec | LegacyPictSpec | dict[str, Any],
    *,
    backend: str | None = None,
    format: OutputFormat = "svg",
) -> str:
    """Shorthand for ``Pict(backend=...).render(spec)``."""
    return Pict(backend=backend, format=format).render(spec)
