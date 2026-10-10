"""EdgePlan helpers — structural validate/build only on Python.

Host ``coord_gen`` (SMILES → coords + align) runs in Rust / JS. The former
Python RDKit processor and free-layout fallbacks were removed.
"""

from __future__ import annotations

from xpict.contracts.edge import (
    AlignOpts,
    CoordGenTask,
    EdgePlan,
    EdgeResult,
    MolTemplate,
)
from xpict.contracts.edge_dict import EdgePlanDict

EdgePlanLike = EdgePlan | EdgePlanDict | str


def validate_edge_plan(plan: EdgePlanLike) -> EdgePlan:
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
        version=1,
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
        ],
    )


def process_edge_plan(plan: EdgePlanLike) -> EdgeResult:
    """Removed: Python RDKit ``coord_gen`` processor.

    Use the Rust crate / JS host processor, or CoordGen + ``align`` once the
    native SMILES bridge lands.
    """
    _ = plan
    raise NotImplementedError(
        "Python process_edge_plan was removed (no RDKit / free-layout fallback). "
        "Use the Rust or JS EdgePlan processor."
    )


def process_edge_plan_with_frames(
    plan: EdgePlanLike,
) -> tuple[EdgeResult, dict[str, str]]:
    """Removed with :func:`process_edge_plan`."""
    _ = plan
    raise NotImplementedError(
        "Python process_edge_plan_with_frames was removed "
        "(no RDKit / free-layout fallback)."
    )
