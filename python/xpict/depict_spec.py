"""Declarative DepictSpec — Rust-first two/three-pass (parity with JS / Rust).

Pass 1: core ``plan_edge`` → EdgePlan
Host:   ``process_edge_plan_with_frames`` (RDKit)
Pass 2: core ``render_doc`` → per-mol scenes
Pass 3: core ``compose_scheme`` → one Scene (reaction_scheme only)
"""

from __future__ import annotations

import json
from typing import Any

from xpict.client import Rendered, SvgAtom, SvgBond, to_svg
from xpict.contracts import DepictSpec
from xpict.contracts.depict import MolNode, ReactionSchemeNode
from xpict.contracts.scene import Scene
from xpict.edge_plan import process_edge_plan_with_frames
from xpict.native_bridge import compose_scheme, plan_edge, render_doc


def _as_depict_spec(spec: DepictSpec | dict[str, Any] | str) -> DepictSpec:
    if isinstance(spec, DepictSpec):
        return spec
    if isinstance(spec, str):
        return DepictSpec.model_validate(json.loads(spec))
    return DepictSpec.model_validate(spec)


def _structure(node: MolNode) -> str:
    for field in (node.molfile, node.cxsmiles, node.smiles):
        if field is not None and str(field).strip():
            return str(field)
    raise ValueError("mol node needs smiles, cxsmiles, or molfile")


def _node_by_id(spec: DepictSpec) -> dict[str, MolNode]:
    nodes = spec.mols()
    out: dict[str, MolNode] = {}
    for i, n in enumerate(nodes):
        key = (n.id or "").strip() or f"m_{i}"
        out[key] = n
    return out


def _is_scheme(doc: DepictSpec) -> bool:
    root = getattr(doc, "root", doc)
    return isinstance(root, ReactionSchemeNode) or getattr(root, "type", None) == "reaction_scheme"


def _rendered_from_row(
    row: dict[str, Any],
    *,
    by_id: dict[str, MolNode],
    frames: dict[str, str],
    source_fallback: str | None = None,
) -> Rendered:
    rid = str(row["id"])
    node = by_id.get(rid)
    source = (
        _structure(node)
        if node is not None
        else (source_fallback or rid)
    )
    molecule = row["molecule"]
    scene = Scene.model_validate(row["scene"])
    atoms = molecule.get("atoms") or []
    bonds_in = molecule.get("bonds") or []
    coords = [
        SvgAtom(
            index=int(a["index"]),
            element=str(a.get("element") or "C"),
            x=float(a["x"]),
            y=float(a["y"]),
            label=a.get("label"),
            charge=int(a.get("charge") or 0),
        )
        for a in atoms
    ]
    bonds = [
        SvgBond(
            index=int(b["index"]),
            begin=int(b["begin"]),
            end=int(b["end"]),
            order=float(b["order"]),
            stereo=b.get("stereo"),
        )
        for b in bonds_in
    ]
    return Rendered(
        width=float(scene.width),
        height=float(scene.height),
        scene=scene,
        molecule=molecule if isinstance(molecule, dict) else dict(molecule),
        source=source,
        frame_molblock=frames.get(rid, ""),
        coords=coords,
        svg_coords=list(coords),
        bonds=bonds,
    )


def depict(spec: DepictSpec | dict[str, Any] | str) -> list[Rendered]:
    """Declarative document → ``list[Rendered]`` via core plan/paint + RDKit edge.

    ``reaction_scheme`` roots are composed into **one** ``Rendered`` (ELK layout
    + edge overlays in Rust). ``mol`` / ``group`` stay one row per molecule.
    """
    doc = _as_depict_spec(spec)
    plan = plan_edge(doc)
    if plan is None:
        return []
    edge, frames = process_edge_plan_with_frames(plan)
    painted = render_doc(doc, edge)
    by_id = _node_by_id(doc)

    if _is_scheme(doc):
        composed = compose_scheme(doc, painted)
        # Prefer first mol as source identity for the composed document.
        first = painted[0] if painted else {"id": "scheme", "molecule": {}}
        return [
            _rendered_from_row(
                {
                    "id": str(first.get("id") or "scheme"),
                    "molecule": first.get("molecule") or {},
                    "scene": composed,
                },
                by_id=by_id,
                frames=frames,
                source_fallback="reaction_scheme",
            )
        ]

    out: list[Rendered] = []
    for row in painted:
        out.append(_rendered_from_row(row, by_id=by_id, frames=frames))
    return out


def depict_svg(spec: DepictSpec | dict[str, Any] | str) -> str:
    """Two/three-pass depict then SVG (one composed scene for schemes)."""
    rows = depict(spec)
    if not rows:
        return ""
    if len(rows) == 1:
        return to_svg(rows[0].scene)
    return "\n".join(to_svg(r.scene) for r in rows)


def is_live_depict_spec(obj: Any) -> bool:
    """True for nested live DepictSpec (``mol|group|reaction_scheme``)."""
    if isinstance(obj, DepictSpec):
        return True
    if isinstance(obj, dict):
        t = obj.get("type")
        return t in ("mol", "group", "reaction_scheme")
    return False
