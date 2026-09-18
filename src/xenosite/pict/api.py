"""Public Pict / render API."""

from __future__ import annotations

from typing import Any, Literal

from xenosite.pict.align import align_layouts
from xenosite.pict.backends import BACKEND_PREFERENCE, get_backend
from xenosite.pict.contracts.layout import LayoutResult, MoleculeLayout
from xenosite.pict.contracts.spec import PictSpec
from xenosite.pict.diagram.elk import layout_diagram_ex
from xenosite.pict.draw.scene_builder import build_scene
from xenosite.pict.draw.svg import scene_to_html, scene_to_svg

OutputFormat = Literal["svg", "html"]


def _resolve_backend_name(requested: str | None) -> str:
    """Default: Indigo when installed, else native stub.

    Multi-engine preference ladders are intentionally gone — native depiction is
    the product goal; Indigo is the one transitional layout engine.
    """
    if requested:
        return requested.lower()
    for name in BACKEND_PREFERENCE:
        if name == "native":
            return "native"
        if name == "indigo":
            try:
                import indigo  # noqa: F401

                return "indigo"
            except ImportError:
                continue
    return "native"


class Pict:
    """Configured depiction engine.

    Backend and output options are runtime config — not part of PictSpec JSON.
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
        spec: PictSpec | dict[str, Any],
        *,
        format: OutputFormat | None = None,
    ) -> str:
        doc = spec if isinstance(spec, PictSpec) else PictSpec.model_validate(spec)
        backend = get_backend(self.backend)
        layouts: list[MoleculeLayout] = [backend.layout(m) for m in doc.molecules]
        layouts = align_layouts(layouts, enabled=doc.diagram.align)
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

    def layout(self, spec: PictSpec | dict[str, Any]) -> LayoutResult:
        doc = spec if isinstance(spec, PictSpec) else PictSpec.model_validate(spec)
        backend = get_backend(self.backend)
        return LayoutResult(molecules=[backend.layout(m) for m in doc.molecules])


def render(
    spec: PictSpec | dict[str, Any],
    *,
    backend: str | None = None,
    format: OutputFormat = "svg",
) -> str:
    """Shorthand for ``Pict(backend=...).render(spec)``."""
    return Pict(backend=backend, format=format).render(spec)
