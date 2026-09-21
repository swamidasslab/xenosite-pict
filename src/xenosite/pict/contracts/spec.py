"""Declarative PictSpec — language-neutral input contract (Pydantic source of truth)."""

from __future__ import annotations

from enum import Enum
from typing import Annotated, Any, Literal

from pydantic import (
    BaseModel,
    BeforeValidator,
    ConfigDict,
    Field,
    WithJsonSchema,
    model_validator,
)

from xenosite.pict.contracts.shorthand import (
    LABEL_DEFAULTS,
    expand_label,
    expand_pict_input,
)


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class DiagramKind(str, Enum):
    single = "single"
    grid = "grid"
    network = "network"
    reaction = "reaction"
    html = "html"


class MarkKind(str, Enum):
    atoms = "atoms"
    bonds = "bonds"
    substructure = "substructure"


class AnnotKind(str, Enum):
    """How an annotation is drawn."""

    callout = "callout"  # indicator arrow and/or text next to a target
    box = "box"  # axis-aligned box around atoms
    oval = "oval"  # ellipse around atoms
    spline = "spline"  # smooth closed loop around atoms


class AnnotPrefer(str, Enum):
    """Preferred callout placement; collision grid may pick another free slot."""

    auto = "auto"
    left = "left"
    right = "right"
    top = "top"
    bottom = "bottom"


class LabelPos(str, Enum):
    """Where a molecule caption sits relative to the drawing."""

    bottom = "bottom"
    top = "top"
    left = "left"
    right = "right"


class LabelSpec(StrictModel):
    """Expanded molecule caption.

    Input shorthand (expanded at the API boundary):

    - ``"ethanol"`` → defaults (``pos=bottom``)
    - ``{"text": "ethanol"}`` → same
    - ``{"text": "ethanol", "pos": "top"}`` → override position
    """

    text: str
    pos: LabelPos = LabelPos.bottom


def _coerce_label(value: Any) -> Any:
    """String / partial dict → expanded LabelSpec-shaped dict; None stays None."""
    if value is None or isinstance(value, LabelSpec):
        return value
    return expand_label(value)


# JSON Schema documents the shorthand union; runtime type is always LabelSpec.
_LABEL_JSON_SCHEMA = {
    "anyOf": [
        {"type": "string"},
        {
            "type": "object",
            "properties": {
                "text": {"type": "string"},
                "pos": {
                    "type": "string",
                    "enum": ["bottom", "top", "left", "right"],
                    "default": "bottom",
                },
            },
            "required": ["text"],
            "additionalProperties": False,
        },
        {"type": "null"},
    ],
    "description": (
        "Molecule caption. A bare string uses defaults (pos=bottom). "
        "A dict with only text is equivalent; other keys override defaults. "
        "Legacy field name `title` is accepted as an alias. "
        "Text supports light markup: TeX-like symbols (\\alpha → α) and "
        "bold/italic via **…** / *…* or \\textbf{} / \\textit{}."
    ),
}

_LabelInput = Annotated[
    LabelSpec | None,
    BeforeValidator(_coerce_label),
    WithJsonSchema(_LABEL_JSON_SCHEMA),
]


class MarkSpec(StrictModel):
    """Legacy publication-style mark (circles / bond strokes / hull).

    Prefer :class:`AnnotationSpec` for callouts, region shapes, and labels.
    """

    kind: MarkKind = MarkKind.atoms
    atoms: list[int] | None = Field(
        default=None, description="0-based atom indices to mark"
    )
    bonds: list[tuple[int, int]] | None = Field(
        default=None, description="0-based atom-index pairs for bonds to mark"
    )
    color: str | None = Field(default=None, description="CSS color for the mark")
    label: str | None = None


class AnnotationSpec(StrictModel):
    """Molecule annotation with collision-aware label placement.

    Targets (use one):

    - ``atoms`` — one atom (callout) or a set (region)
    - ``bonds`` — bond endpoints as ``[begin, end]`` pairs
    - ``ring`` — atom indices of a ring (callout at ring center)

    Kinds:

    - ``callout`` — indicator arrow and/or text next to the target
    - ``box`` / ``oval`` / ``spline`` — region outline around ``atoms``
    """

    kind: AnnotKind = AnnotKind.callout
    atoms: list[int] | None = Field(
        default=None, description="0-based atom indices (target or region)"
    )
    bonds: list[tuple[int, int]] | None = Field(
        default=None, description="0-based atom-index pairs"
    )
    ring: list[int] | None = Field(
        default=None,
        description="Ordered atom indices of a ring (callout at centroid)",
    )
    label: str | None = Field(
        default=None,
        description="Annotation caption (supports light TeX/markdown markup)",
    )
    color: str | None = Field(default=None, description="Stroke/fill color")
    arrow: bool = Field(
        default=True, description="Draw indicator arrow for callout annotations"
    )
    prefer: AnnotPrefer = Field(
        default=AnnotPrefer.auto,
        description="Preferred callout/label side; grid may choose another free slot",
    )

    @model_validator(mode="after")
    def require_target(self) -> AnnotationSpec:
        if not any([self.atoms, self.bonds, self.ring]):
            raise ValueError("AnnotationSpec needs atoms, bonds, or ring")
        if self.kind in (AnnotKind.box, AnnotKind.oval, AnnotKind.spline):
            if not self.atoms or len(self.atoms) < 1:
                raise ValueError(f"{self.kind.value} annotation needs atoms")
        return self


class ShadeSpec(StrictModel):
    """Continuous per-atom / per-bond scores for colormap shading."""

    atoms: list[float] | None = Field(
        default=None, description="Per-atom scores (same length as atom count)"
    )
    bonds: list[float] | None = Field(
        default=None, description="Per-bond scores"
    )
    colormap: str = Field(default="xenosite", description="Named colormap")
    vmin: float | None = None
    vmax: float | None = None


class MoleculeSpec(StrictModel):
    """One molecule in a depiction document."""

    id: str | None = Field(default=None, description="Stable id for diagram edges/nodes")
    smiles: str | None = Field(default=None, description="SMILES string")
    cxsmiles: str | None = Field(
        default=None,
        description="ChemAxon extended SMILES (CXSMILES), including coords/labels when present",
    )
    esmiles: str | None = Field(
        default=None,
        description="MolParser E-SMILES (SMILES<sep>EXTENSION); schema hook for Markush",
    )
    molfile: str | None = Field(default=None, description="Optional MDL molblock")
    label: _LabelInput = Field(
        default=None,
        description=(
            "Molecule caption. A bare string uses defaults (pos=bottom). "
            "A dict with only text is equivalent; other keys override defaults. "
            "Legacy field name `title` is accepted as an alias. "
            "Text supports light markup: TeX-like symbols (\\alpha → α) and "
            "bold/italic via **…** / *…* or \\textbf{} / \\textit{}."
        ),
    )
    marks: list[MarkSpec] = Field(default_factory=list)
    annotations: list[AnnotationSpec] = Field(
        default_factory=list,
        description=(
            "Callouts, region shapes (box/oval/spline), and labels. "
            "Placement uses the molecule collision grid."
        ),
    )
    shade: ShadeSpec | None = None
    color: str | None = Field(default=None, description="Bond/backbone color override")

    @model_validator(mode="before")
    @classmethod
    def _alias_title_to_label(cls, data: Any) -> Any:
        if isinstance(data, dict) and "title" in data:
            data = dict(data)
            if "label" not in data or data["label"] is None:
                data["label"] = data["title"]
            del data["title"]
        return data

    @model_validator(mode="after")
    def require_structure(self) -> MoleculeSpec:
        if not any([self.smiles, self.cxsmiles, self.esmiles, self.molfile]):
            raise ValueError(
                "MoleculeSpec requires smiles, cxsmiles, esmiles, or molfile"
            )
        return self


class EdgeArrow(str, Enum):
    """Arrow head / shaft style for diagram edges."""

    forward = "forward"  # single →
    equilibrium = "equilibrium"  # ⇌ stacked half-arrows
    open = "open"  # ⇒ hollow head (retrosynthetic-style)
    line = "line"  # connector without arrowhead


class EdgeSpec(StrictModel):
    """Edge between molecule nodes in a network / reaction diagram."""

    source: str
    target: str
    label: str | None = None
    role: str | None = Field(
        default=None,
        description="Optional semantic role (e.g. enzyme, inhibits) — not drawn by default",
    )
    arrow: EdgeArrow = EdgeArrow.forward
    color: str | None = Field(default=None, description="Stroke/fill color for the arrow")
    stroke_width: float | None = Field(default=None, description="Shaft stroke width")
    dashed: bool = Field(default=False, description="Dashed shaft (e.g. hypothetical step)")


class DiagramSpec(StrictModel):
    """Multi-molecule composition options (ELK / grid / HTML)."""

    kind: DiagramKind = DiagramKind.single
    columns: int | None = Field(default=None, description="Grid columns when kind=grid")
    edges: list[EdgeSpec] = Field(default_factory=list)
    elk_options: dict[str, Any] = Field(
        default_factory=dict,
        description="Pass-through ELK layout options (engine-specific support varies)",
    )
    align: bool = Field(
        default=False,
        description=(
            "Align later molecules onto the first. RDKit fixes the matched atoms "
            "and redraws the rest when it is installed; otherwise each molecule "
            "is depicted on its own and rigidly rotated and translated onto the match."
        ),
    )


class PictSpec(StrictModel):
    """Declarative depiction document.

    Engine choice (`backend`) is runtime config on Pict/render — not part of this document.
    Shorthand fields (e.g. molecule labels) are expanded at validation; the library
    then works only on fully expanded models.
    """

    molecules: Annotated[list[MoleculeSpec], Field(min_length=1)]
    diagram: DiagramSpec = Field(default_factory=DiagramSpec)
    width: float | None = Field(default=None, description="Preferred SVG width")
    height: float | None = Field(default=None, description="Preferred SVG height")
    halo: bool = Field(
        default=True,
        description=(
            "Global white knockout under all ink (bonds, labels, marks, captions, "
            "edge labels). On = everything; off = nothing. Halos live on the "
            "bottom layer so they never obscure drawn content."
        ),
    )
    meta: dict[str, Any] = Field(default_factory=dict)

    @model_validator(mode="before")
    @classmethod
    def _expand_shorthand(cls, data: Any) -> Any:
        if isinstance(data, dict):
            return expand_pict_input(data)
        return data


def expand_pict(spec: PictSpec | dict[str, Any]) -> PictSpec:
    """Validate and fully expand a PictSpec input."""
    if isinstance(spec, PictSpec):
        return spec
    return PictSpec.model_validate(spec)


def compress_pict(spec: PictSpec | dict[str, Any]) -> dict[str, Any]:
    """Dump a PictSpec to JSON-shaped dict with shorthand compression."""
    from xenosite.pict.contracts.shorthand import compress_pict_input

    if isinstance(spec, PictSpec):
        data = spec.model_dump(mode="json")
    else:
        data = dict(spec)
    return compress_pict_input(data)


# Re-export defaults for callers / schema docs.
__all__ = [
    "AnnotKind",
    "AnnotPrefer",
    "AnnotationSpec",
    "DiagramKind",
    "DiagramSpec",
    "EdgeArrow",
    "EdgeSpec",
    "LABEL_DEFAULTS",
    "LabelPos",
    "LabelSpec",
    "MarkKind",
    "MarkSpec",
    "MoleculeSpec",
    "PictSpec",
    "ShadeSpec",
    "compress_pict",
    "expand_pict",
]
