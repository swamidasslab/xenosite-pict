# Auto-generated from Rust schemars (make types) — do not edit.
"""EdgePlan / EdgeResult — live ABI from xpict-core (schemars). TypedDict wire forms (no ``dict[str, Any]``)."""

from __future__ import annotations

from typing import Literal, NotRequired, Required, TypedDict

AlignMode = Literal['exact', 'greedy', 'skip']


class AlignOptsDict(TypedDict, total=False):
    """Align this mol onto its parent template."""
    atom_map: NotRequired[list[tuple[int, int]]]
    min_atoms: NotRequired[int]
    mode: NotRequired[AlignMode]
    timeout_ms: NotRequired[int]


class MolTemplateDict(TypedDict, total=False):
    """One node in a coord_gen forest (root = free layout; children align to parent)."""
    id: Required[str]
    align: NotRequired[AlignOptsDict]
    cxsmiles: NotRequired[str]
    molfile: NotRequired[str]
    smiles: NotRequired[str]
    template_for: NotRequired[list[MolTemplateDict]]


class CoordGenTaskDict(TypedDict, total=False):
    type: Required[Literal['coord_gen']]
    roots: NotRequired[list[MolTemplateDict]]


EdgeTaskDict = CoordGenTaskDict


class EdgePlanDict(TypedDict, total=False):
    """Host callback request."""
    version: Required[int]
    tasks: NotRequired[list[EdgeTaskDict]]


CoordMethod = Literal['free', 'atom_map', 'mcs', 'greedy', 'skip', 'none']


AlignDebugRole = Literal['template', 'query']


class AlignDebugDict(TypedDict, total=False):
    """Stamped alignment correspondence for [`MoleculeIn::debug_align`] paint."""
    role: Required[AlignDebugRole]
    peer_id: NotRequired[str]
    tags: NotRequired[list[tuple[int, int]]]


AromaticStyle = Literal['circle', 'kekule']


class AtomInDict(TypedDict, total=False):
    """Atom input for a molecule depict call (SVG-space coords from the caller)."""
    index: Required[int]
    x: Required[float]
    y: Required[float]
    charge: NotRequired[int]
    element: NotRequired[str]
    label: NotRequired[str]
    mapno: NotRequired[int]
    radical: NotRequired[int]
    z: NotRequired[int]


class BondInDict(TypedDict, total=False):
    """Bond input (indices into [`AtomIn::index`])."""
    begin: Required[int]
    end: Required[int]
    index: Required[int]
    order: Required[float]
    interior: NotRequired[tuple[float, float]]
    stereo: NotRequired[str]


Attach = Literal['any', 'all']


class SpokeInDict(TypedDict, total=False):
    """Segment drawn from a ring centroid to a substituent (CX ``m:``)."""
    atom: Required[int]
    endpts: Required[list[int]]
    attach: NotRequired[Attach]
    dummy: NotRequired[int]


class MoleculeInDict(TypedDict, total=False):
    """One molecule ready to paint (coords already in SVG / ``SCALE`` space)."""
    atoms: Required[list[AtomInDict]]
    bonds: Required[list[BondInDict]]
    align_debug: NotRequired[AlignDebugDict]
    aromatic: NotRequired[AromaticStyle]
    atom_shade: NotRequired[list[float]]
    bond_shade: NotRequired[list[float]]
    color: NotRequired[str]
    debug_align: NotRequired[bool]
    debug_box: NotRequired[bool]
    id: NotRequired[str]
    scale: NotRequired[float]
    shade_vmax: NotRequired[float]
    shade_vmin: NotRequired[float]
    spokes: NotRequired[list[SpokeInDict]]
    weight: NotRequired[float]


class CoordGenMoleculeResultDict(TypedDict, total=False):
    """One flat molecule entry in a coord_gen result."""
    id: Required[str]
    method: Required[CoordMethod]
    ok: Required[bool]
    error: NotRequired[str]
    molecule: NotRequired[MoleculeInDict]
    used_map: NotRequired[list[tuple[int, int]]]


class CoordGenTaskResultDict(TypedDict, total=False):
    type: Required[Literal['coord_gen']]
    molecules: Required[list[CoordGenMoleculeResultDict]]
    ok: Required[bool]


EdgeTaskResultDict = CoordGenTaskResultDict


class EdgeResultDict(TypedDict, total=False):
    version: Required[int]
    results: NotRequired[list[EdgeTaskResultDict]]
