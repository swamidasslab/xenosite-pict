"""EdgePlan build + host ``coord_gen`` processor (Python / RDKit)."""

from __future__ import annotations

from typing import Any

from xpict.contracts.edge import (
    AlignOpts,
    CoordGenMoleculeResult,
    CoordGenTask,
    CoordGenTaskResult,
    EdgePlan,
    EdgeResult,
    MolTemplate,
)

# Keep in sync with ``xpict_core::edge::MIN_MCS_ATOMS``.
_MIN_MCS_ATOMS = 3


def validate_edge_plan(plan: EdgePlan | dict[str, Any]) -> EdgePlan:
    """Structural checks via Rust core (unique ids, roots, structure fields)."""
    from xpict.native_bridge import validate_edge_plan as _core_validate

    return EdgePlan.model_validate(_core_validate(plan))


def build_align_plan(
    *,
    template_source: str,
    query_source: str,
    atom_map: list[tuple[int, int]] | None = None,
    template_id: str = "m_0",
    query_id: str = "m_1",
) -> EdgePlan:
    """Two-node forest: template root + one query child (simple-client helper)."""
    child_align = AlignOpts(atom_map=atom_map) if atom_map is not None else AlignOpts()
    return EdgePlan(
        tasks=[
            CoordGenTask(
                roots=[
                    MolTemplate(
                        id=template_id,
                        smiles=template_source,
                        align=None,
                        template_for=[
                            MolTemplate(
                                id=query_id,
                                smiles=query_source,
                                align=child_align,
                                template_for=[],
                            )
                        ],
                    )
                ]
            )
        ]
    )


def _source_of(node: MolTemplate) -> str:
    for v in (node.smiles, node.cxsmiles, node.molfile):
        if v is not None and str(v).strip():
            return str(v)
    raise ValueError(f"MolTemplate {node.id}: empty structure")


def _layout(
    source: str,
    *,
    template: str | None,
    atom_map: list[tuple[int, int]] | None,
    id: str | None,
    min_atoms: int,
) -> tuple[dict[str, Any], str, str, list[tuple[int, int]] | None]:
    """Returns molecule, pose_molblock, method, used_map."""
    from xpict.client import layout_with_rdkit

    molecule, pose, meta = layout_with_rdkit(
        source,
        template=template,
        id=id,
        atom_map=atom_map,
        min_atoms=min_atoms,
    )
    return molecule, pose, meta["method"], meta.get("used_map")


def process_edge_plan(plan: EdgePlan | dict[str, Any]) -> EdgeResult:
    """Run all ``coord_gen`` tasks; always return a flat molecule list.

    Align failure → automatic free (unaligned) layout for that mol; ``method``
    is ``none`` and ``ok`` stays true when coords were produced.
    """
    p = validate_edge_plan(plan)
    task_results: list[CoordGenTaskResult] = []

    for task in p.tasks:
        rows: list[CoordGenMoleculeResult] = []
        poses: dict[str, str] = {}

        def visit(node: MolTemplate, parent_id: str | None) -> None:
            source = _source_of(node)
            min_atoms = _MIN_MCS_ATOMS
            atom_map = None
            if node.align is not None:
                if node.align.min_atoms is not None:
                    min_atoms = int(node.align.min_atoms)
                atom_map = node.align.atom_map

            template_pose = poses.get(parent_id) if parent_id else None

            def free_layout() -> tuple[dict[str, Any], str]:
                molecule, pose, _method, _used = _layout(
                    source,
                    template=None,
                    atom_map=None,
                    id=node.id,
                    min_atoms=min_atoms,
                )
                return molecule, pose

            try:
                if parent_id is None:
                    molecule, pose = free_layout()
                    rows.append(
                        CoordGenMoleculeResult(
                            id=node.id,
                            ok=True,
                            method="free",
                            used_map=None,
                            molecule=molecule,
                            error=None,
                        )
                    )
                    poses[node.id] = pose
                else:
                    if template_pose is None:
                        raise RuntimeError(f"missing parent pose {parent_id}")
                    molecule, pose, method, used = _layout(
                        source,
                        template=template_pose,
                        atom_map=atom_map,
                        id=node.id,
                        min_atoms=min_atoms,
                    )
                    if method in ("atom_map", "mcs"):
                        rows.append(
                            CoordGenMoleculeResult(
                                id=node.id,
                                ok=True,
                                method=method,  # type: ignore[arg-type]
                                used_map=used,
                                molecule=molecule,
                                error=None,
                            )
                        )
                    else:
                        # Align soft-fail already returned free coords from layout;
                        # re-run unaligned to guarantee a clean free pose.
                        molecule, pose = free_layout()
                        rows.append(
                            CoordGenMoleculeResult(
                                id=node.id,
                                ok=True,
                                method="none",
                                used_map=None,
                                molecule=molecule,
                                error="align failed; fell back to unaligned coord gen",
                            )
                        )
                    poses[node.id] = pose
            except Exception as e:  # noqa: BLE001 — try free fallback, then record
                try:
                    molecule, pose = free_layout()
                    rows.append(
                        CoordGenMoleculeResult(
                            id=node.id,
                            ok=True,
                            method="none" if parent_id is not None else "free",
                            used_map=None,
                            molecule=molecule,
                            error=(
                                f"align/layout error ({e}); fell back to unaligned coord gen"
                                if parent_id is not None
                                else None
                            ),
                        )
                    )
                    poses[node.id] = pose
                except Exception as e2:  # noqa: BLE001
                    rows.append(
                        CoordGenMoleculeResult(
                            id=node.id,
                            ok=False,
                            method="none",
                            used_map=None,
                            molecule=None,
                            error=str(e2),
                        )
                    )
                    return

            for child in node.template_for:
                visit(child, node.id)

        for root in task.roots:
            visit(root, None)

        task_ok = all(r.ok for r in rows)
        task_results.append(
            CoordGenTaskResult(ok=task_ok, molecules=rows)
        )

    return EdgeResult(results=task_results)
