"""Single-molecule client — parity with JS ``xpict.mol`` / ``render`` / ``toSvg``.

Python builds a DepictSpec; Rust ``depict`` paints. No host cheminformatics.
"""

from __future__ import annotations

from dataclasses import dataclass

from xpict.base import OptsKeywords, PictBase
from xpict.contracts.depict import AlignTo, AlignToSpec, AromaticStyle, GroupNode, MolNode
from xpict.contracts.depict import DepictSpec as WireDepictSpec
from xpict.contracts.scene import Scene
from xpict.contracts.scene_dict import SceneDict
from xpict.display import render_depict_html, render_depict_mols
from xpict.draw.svg import scene_to_svg as _scene_to_svg
from xpict.mol_source import source_to_mol_fields
from xpict.rendered import Rendered, SvgAtom, SvgBond
from xpict.sugar import mols_of

__all__ = [
    "Mol",
    "MolRenderOptions",
    "Rendered",
    "SvgAtom",
    "SvgBond",
    "mol",
    "render",
    "render_depict_html",
    "render_depict_mols",
    "to_svg",
]


@dataclass
class Mol(PictBase):
    """Input molecule — SMILES / CXSMILES / molfile plus optional align frame."""

    source: str
    frame_molblock: str | None = None
    id: str | None = None
    align_to: str | None = None

    def render(self, opts: MolRenderOptions | None = None) -> Rendered:
        return render(self, opts)

    def ensure_frame(self) -> str:
        if not self.frame_molblock:
            self.frame_molblock = render(self).frame_molblock
        return self.frame_molblock

    def to_node(self, id: str | None = None) -> MolNode:
        nid = self.id if id is None else id
        return MolNode.model_validate(
            {
                "id": nid,
                "align_to": self.align_to,
                **source_to_mol_fields(self.source),
            }
        )

    def to_spec(self) -> WireDepictSpec:
        from xpict.spec import DepictSpec as HostDepictSpec

        return HostDepictSpec(self.to_node())

    def _repr_html_(self) -> str:
        return render_depict_html(self.to_spec())

    def _repr_svg_(self) -> str | None:
        rows = render_depict_mols(self.to_spec())
        if len(rows) == 1:
            return rows[0].to_svg()
        return None

    def _repr_mimebundle_(
        self,
        include: list[str] | None = None,
        exclude: list[str] | None = None,
    ) -> dict[str, str]:
        _ = include, exclude
        spec = self.to_spec()
        return {
            "text/html": self._repr_html_(),
            "text/plain": f"Mol→DepictSpec({spec.root.type}, {len(mols_of(spec))} mols)",
        }


@dataclass
class MolRenderOptions:
    """Render options — parity with JS / Rust ``MolRenderOptions``."""

    id: str | None = None
    color: str | None = None
    atom_shade: list[float] | None = None
    bond_shade: list[float] | None = None
    mark_atoms: list[int] | None = None
    mark_bonds: list[tuple[int, int]] | None = None
    star_labels: list[str | None] | None = None
    weight: float | None = None
    scale: float | None = None
    aromatic: AromaticStyle | None = None
    debug_box: bool | None = None
    debug_align: bool | None = None
    align_to: Mol | Rendered | None = None
    atom_map: list[tuple[int, int]] | None = None


def mol(
    smiles_or_molfile: str,
    *,
    id: str | None = None,
    align_to: str | None = None,
) -> Mol:
    """Construct a ``Mol`` (JS / Rust ``xpict.mol``)."""
    text = smiles_or_molfile.strip()
    if not text:
        raise ValueError("mol() requires a non-empty SMILES or molfile")
    return Mol(source=text, id=id, align_to=align_to)


def to_svg(scene: Scene | SceneDict) -> str:
    """Scene JSON / ``Scene`` → SVG string (JS ``xpict.toSvg``)."""
    if isinstance(scene, dict):
        scene = Scene.model_validate(scene)
    return _scene_to_svg(scene)


def _opts_keywords(options: MolRenderOptions) -> OptsKeywords:
    out: OptsKeywords = {}
    for key in (
        "color",
        "weight",
        "scale",
        "aromatic",
        "debug_box",
        "debug_align",
    ):
        val = getattr(options, key)
        if val is not None:
            out[key] = val  # type: ignore[literal-required]
    return out


def _align_to_wire(options: MolRenderOptions) -> AlignTo | None:
    if options.align_to is None:
        return None
    if options.atom_map is not None:
        ref = options.align_to
        if isinstance(ref, Rendered):
            back = ref.mol
            rid = (
                (back.id if isinstance(back, Mol) else None)
                or ref.molecule.id
                or "m_0"
            )
        else:
            rid = ref.id or "m_0"
        return AlignToSpec(
            ref=str(rid),
            atom_map=[(int(q), int(r)) for q, r in options.atom_map],
        )
    if isinstance(options.align_to, Rendered):
        back = options.align_to.mol
        if isinstance(back, Mol) and back.id:
            return str(back.id)
        mid = options.align_to.molecule.id
        return str(mid) if mid else "m_0"
    return options.align_to.id or "m_0"


def _spec_for_render(m: Mol, options: MolRenderOptions) -> WireDepictSpec:
    from xpict.contracts.depict import ShadeSpec
    from xpict.spec import DepictSpec

    query_fields = source_to_mol_fields(m.source)
    qid = options.id or m.id or "m_1"
    kw = _opts_keywords(options)
    shade = None
    if options.atom_shade is not None or options.bond_shade is not None:
        shade = ShadeSpec(
            atoms=options.atom_shade,
            bonds=options.bond_shade,
        )

    if options.align_to is None:
        payload = {
            "id": options.id or m.id,
            "align_to": m.align_to,
            "shade": shade,
            "star_labels": options.star_labels,
            **query_fields,
            **{k: v for k, v in kw.items() if k in MolNode.model_fields},
        }
        return DepictSpec(MolNode.model_validate(payload))

    if isinstance(options.align_to, Rendered):
        back = options.align_to.mol
        ref_mol = back if isinstance(back, Mol) else mol(options.align_to.source)
    else:
        ref_mol = options.align_to
    ref_id = ref_mol.id or "m_0"
    ref_node = MolNode.model_validate(
        {"id": ref_id, **source_to_mol_fields(ref_mol.source)}
    )
    align_to = _align_to_wire(options)
    query_payload = {
        "id": qid if qid != ref_id else "m_1",
        "align_to": align_to,
        "shade": shade,
        "star_labels": options.star_labels,
        **query_fields,
        **{k: v for k, v in kw.items() if k in MolNode.model_fields},
    }
    query = MolNode.model_validate(query_payload)
    doc = DepictSpec(
        GroupNode(type="group", align=False, children=[ref_node, query])
    )
    return DepictSpec(doc.opts(**kw).root)


def render(input: Mol | str, opts: MolRenderOptions | None = None) -> Rendered:
    """Build a DepictSpec and run Rust ``depict`` → ``Rendered``."""
    from xpict.depict_spec import depict

    m = mol(input) if isinstance(input, str) else input
    options = opts if opts is not None else MolRenderOptions()
    if options.atom_map is not None and options.align_to is None:
        raise ValueError("atom_map requires align_to")

    rows = depict(_spec_for_render(m, options))
    if not rows:
        raise RuntimeError("depict returned no molecules")
    painted = rows[-1]
    painted.mol = m
    if m.frame_molblock is None and options.align_to is None:
        m.frame_molblock = painted.frame_molblock
    return painted
