"""Rust core bindings (``xpict._native``). Extension is required on the ship path."""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

try:
    from xpict import _native as _rust
except ImportError as e:  # pragma: no cover
    raise ImportError(
        "xpict requires the Rust extension (xpict._native). "
        "Build with: maturin develop --manifest-path crates/xpict-py/Cargo.toml"
    ) from e

HAS_RUST_CORE: bool = True

if TYPE_CHECKING:
    from xpict._native import Shape as Shape
else:
    Shape = _rust.Shape


@dataclass(frozen=True, slots=True)
class CapsuleInk:
    """Tagged ink for a line capsule — halo via Rust when available."""

    x1: float
    y1: float
    x2: float
    y2: float
    radius: float


@dataclass(frozen=True, slots=True)
class DiskInk:
    """Tagged ink for a filled disk."""

    cx: float
    cy: float
    radius: float


InkGeometry = Any  # Shape | CapsuleInk | DiskInk


def multi_bond_offset(length: float) -> float:
    return float(_rust.multi_bond_offset(length))


def capsule_halo_path_d(
    x1: float,
    y1: float,
    x2: float,
    y2: float,
    ink_radius: float,
    grow: float,
) -> str | None:
    return _rust.capsule_halo_path_d(x1, y1, x2, y2, ink_radius, grow)


def disk_halo_path_d(cx: float, cy: float, ink_radius: float, grow: float) -> str | None:
    return _rust.disk_halo_path_d(cx, cy, ink_radius, grow)


def elk_layout_json(graph_json: str) -> str | None:
    """Native elkrs layout."""
    fn = getattr(_rust, "elk_layout_json", None)
    if fn is None:
        return None
    return str(fn(graph_json))


def _as_json(obj: Any) -> str:
    if isinstance(obj, str):
        return obj
    dump = getattr(obj, "model_dump", None)
    if callable(dump):
        return json.dumps(dump(mode="json"))
    return json.dumps(obj)


def validate_edge_plan(plan: dict[str, Any] | str | Any) -> dict[str, Any]:
    """Structural checks via Rust core."""
    return json.loads(_rust.validate_edge_plan(_as_json(plan)))


def mcs_atom_map(
    query_smiles: str,
    template_smiles: str,
    min_atoms: int | None = None,
) -> list[tuple[int, int]] | None:
    """House MCS via chematic (not RDKit FMCS). Returns ``(query, template)`` pairs."""
    raw = _rust.mcs_atom_map(query_smiles, template_smiles, min_atoms)
    data = json.loads(raw)
    if data is None:
        return None
    return [(int(q), int(t)) for q, t in data]


def mcs_atom_map_graph(
    query: dict[str, Any] | str,
    template: dict[str, Any] | str,
    min_atoms: int | None = None,
) -> list[tuple[int, int]] | None:
    """House MCS from layout graphs (``{atoms, bonds}``)."""
    raw = _rust.mcs_atom_map_graph(_as_json(query), _as_json(template), min_atoms)
    data = json.loads(raw)
    if data is None:
        return None
    return [(int(q), int(t)) for q, t in data]


def mcs_atom_map_graph_candidates(
    query: dict[str, Any] | str,
    template: dict[str, Any] | str,
    min_atoms: int | None = None,
) -> list[list[tuple[int, int]]]:
    """All MCS embeddings for rigid ranking (``choose_mapping``)."""
    raw = _rust.mcs_atom_map_graph_candidates(
        _as_json(query), _as_json(template), min_atoms
    )
    data = json.loads(raw)
    return [[(int(q), int(t)) for q, t in pairs] for pairs in data]


def plan_edge(spec: dict[str, Any] | str | Any) -> dict[str, Any] | None:
    """Pass 1: DepictSpec → EdgePlan (Rust core). ``None`` when empty."""
    raw = _rust.plan_edge(_as_json(spec))
    return json.loads(raw)


def resolve_edge_plan_maps(plan: dict[str, Any] | str | Any) -> dict[str, Any]:
    """Fill missing align atom maps via chematic MCS (Rust core)."""
    return json.loads(_rust.resolve_edge_plan_maps(_as_json(plan)))


def layout_chematic(source: str, id: str | None = None) -> dict[str, Any]:
    """Experimental chematic 2D layout → ``MoleculeIn`` dict (host flag only)."""
    return json.loads(_rust.layout_chematic(source, id))


def render_doc(
    spec: dict[str, Any] | str | Any, edge: dict[str, Any] | str | Any
) -> list[dict[str, Any]]:
    """Pass 2: DepictSpec + EdgeResult → DocPaint rows (Rust core).

    Chrome (CX / star_labels / shade / color) is applied in core — callers
    only process the EdgePlan between passes.
    """
    return json.loads(_rust.render_doc(_as_json(spec), _as_json(edge)))


def compose_scheme(
    spec: dict[str, Any] | str | Any, paints: list[dict[str, Any]] | str
) -> dict[str, Any]:
    """Pass 3: DepictSpec + DocPaint rows → composed Scene (ELK + overlays)."""
    paints_json = paints if isinstance(paints, str) else json.dumps(paints)
    return json.loads(_rust.compose_scheme(_as_json(spec), paints_json))


def halo_path_d_for_ink(ink: InkGeometry, dist: float) -> str | None:
    """Rust fast path for tagged ink; ``None`` → caller uses Shape.halo."""
    if isinstance(ink, CapsuleInk):
        return capsule_halo_path_d(ink.x1, ink.y1, ink.x2, ink.y2, ink.radius, dist)
    if isinstance(ink, DiskInk):
        return disk_halo_path_d(ink.cx, ink.cy, ink.radius, dist)
    return None
