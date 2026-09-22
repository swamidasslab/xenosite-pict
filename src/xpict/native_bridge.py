"""Optional Rust core (``xpict._native``). fontTools stay required for glyphs."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

try:
    from xpict import _native as _rust
except ImportError as e:  # pragma: no cover - extension required for ship path
    raise ImportError(
        "xpict requires the Rust extension (xpict._native). "
        "Build with: maturin develop --manifest-path crates/xpict-py/Cargo.toml"
    ) from e

HAS_RUST_CORE: bool = True
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


def halo_path_d_for_ink(ink: InkGeometry, dist: float) -> str | None:
    """Rust fast path for tagged ink; ``None`` → caller uses Shape.halo."""
    if isinstance(ink, CapsuleInk):
        return capsule_halo_path_d(ink.x1, ink.y1, ink.x2, ink.y2, ink.radius, dist)
    if isinstance(ink, DiskInk):
        return disk_halo_path_d(ink.cx, ink.cy, ink.radius, dist)
    return None
