"""Type stubs for the optional Rust extension ``xpict._native``."""

from typing import Optional

__version__: str

BOND_PX: float
OFFSET_PX: float
STROKE_PX: float
SHADE_FRAC: float

def multi_bond_offset(length: float) -> float: ...
def centered_displacements(order: float, off: float) -> list[float]: ...
def plotdot_rings(z: float, levels: int = 4) -> list[tuple[float, float]]: ...
def plotdot_disks(
    zs: list[float], coords: list[tuple[float, float]], levels: int = 4
) -> list[tuple[float, float, float, float]]: ...
def capsule_halo_path_d(
    x1: float,
    y1: float,
    x2: float,
    y2: float,
    ink_radius: float,
    grow: float,
) -> Optional[str]: ...
def disk_halo_path_d(
    cx: float, cy: float, ink_radius: float, grow: float
) -> Optional[str]: ...
