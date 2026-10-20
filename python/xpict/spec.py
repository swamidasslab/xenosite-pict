"""Host DepictSpec + single-mol sugar (``mol`` / ``render``)."""

from __future__ import annotations

from dataclasses import dataclass, replace
from xpict.base import OptsKeywords, PictBase
from xpict.contracts.depict import (
    AlignTo,
    AlignToSpec,
    AromaticStyle,
    GroupNode,
    LabelPos,
    MolNode,
    MolTextSpec,
    ShadeSpec,
)
from xpict.contracts.depict import DepictSpec as WireDepictSpec
from xpict.rendered import Rendered


class DepictSpec(PictBase, WireDepictSpec):
    """Live document root — generated wire model + host operators via ``PictBase``."""

    def to_spec(self) -> DepictSpec:
        return self

    def mols(self) -> list[MolNode]:
        from xpict.sugar import mols_of

        return mols_of(self)


@dataclass
class Mol(PictBase):
    """Input molecule — SMILES / CXSMILES / molfile plus optional align frame."""

    source: str
    frame_molblock: str | None = None
    id: str | None = None
    align_to: str | None = None
    #: Caption string → wire ``MolNode.name`` (painted in the mol SVG).
    #: ``label`` remains as host sugar alias for ``name``.
    label: str | None = None
    label_pos: LabelPos | None = None
    label_color: str | None = None
    label_scale: float | None = None

    def render(self, opts: MolRenderOptions | None = None) -> Rendered:
        return render_mol(self, opts)

    def ensure_frame(self) -> str:
        if not self.frame_molblock:
            self.frame_molblock = render_mol(self).frame_molblock
        return self.frame_molblock

    def with_label(
        self,
        text: str,
        *,
        pos: LabelPos | None = None,
        color: str | None = None,
        scale: float | None = None,
    ) -> Mol:
        """Return a copy with a caption (``mol(..., name=...)`` equivalent)."""
        return replace(
            self,
            label=text,
            label_pos=pos,
            label_color=color if color is not None else self.label_color,
            label_scale=scale if scale is not None else self.label_scale,
        )

    def to_node(self, id: str | None = None) -> MolNode:
        """Build a ``MolNode`` from this source (smiles / cxsmiles / molfile)."""
        text = self.source.strip()
        if "V2000" in text or "V3000" in text or text.count("\n") >= 3:
            kind_fields = {"molfile": text}
        elif "|" in text:
            kind_fields = {"cxsmiles": text}
        else:
            kind_fields = {"smiles": text}
        nid = self.id if id is None else id
        node = MolNode.model_validate(
            {"id": nid, "align_to": self.align_to, **kind_fields}
        )
        cap = (self.label or "").strip()
        if not cap:
            return node
        mid = (node.id or "m_0").strip() or "m_0"
        # Privileged wire ``name`` — default side below (handled in Rust).
        if (
            self.label_pos is not None
            or self.label_color is not None
            or self.label_scale is not None
        ):
            name: str | MolTextSpec = MolTextSpec(
                content=cap,
                pos=self.label_pos,
                color=self.label_color,
                scale=self.label_scale,
            )
        else:
            name = cap
        return node.model_copy(update={"id": mid, "name": name})

    def to_caption_nodes(self, id: str | None = None) -> list[MolNode]:
        """Lone mol node (caption lives on ``MolNode.name``, not a sibling text)."""
        return [self.to_node(id=id)]

    def to_spec(self, opts: MolRenderOptions | None = None) -> DepictSpec:
        """Document for paint — identity on the mol; cascade via ``.opts`` bag."""
        if opts is None:
            return DepictSpec(self.to_node())
        shade = opts.shade_scores()

        if opts.align_to is None:
            node = self.to_node(id=opts.id)
            if opts.star_labels is not None:
                node = node.model_copy(update={"star_labels": opts.star_labels})
            if shade is not None:
                node = node.model_copy(update={"shade": shade})
            doc = DepictSpec(node)
        else:
            ref = opts.reference_mol()
            ref_id = ref.id or "m_0"
            qid = opts.id or self.id or "m_1"
            if qid == ref_id:
                qid = "m_1"
            query = self.to_node(id=qid).model_copy(
                update={
                    "align_to": opts.align_wire(),
                    **(
                        {"star_labels": opts.star_labels}
                        if opts.star_labels is not None
                        else {}
                    ),
                    **({"shade": shade} if shade is not None else {}),
                }
            )
            doc = DepictSpec(
                GroupNode(
                    type="group",
                    align=False,
                    children=[ref.to_node(id=ref_id), query],
                )
            )

        kw = opts.cascade_keywords()
        return DepictSpec(doc.opts(**kw).root) if kw else doc


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

    def cascade_keywords(self) -> OptsKeywords:
        """Universal ``MolOpts`` keys for the document ``opts`` bag."""
        out: OptsKeywords = {}
        for key in (
            "color",
            "weight",
            "scale",
            "aromatic",
            "debug_box",
            "debug_align",
        ):
            val = getattr(self, key)
            if val is not None:
                out[key] = val  # type: ignore[literal-required]
        return out

    def shade_scores(self) -> ShadeSpec | None:
        """Non-cascading per-atom / per-bond scores (leaf ``MolNode.shade``)."""
        if self.atom_shade is None and self.bond_shade is None:
            return None
        return ShadeSpec(atoms=self.atom_shade, bonds=self.bond_shade)

    def reference_mol(self) -> Mol:
        """``align_to`` as a ``Mol`` (unwraps ``Rendered``)."""
        if self.align_to is None:
            raise ValueError("align_to is required")
        if isinstance(self.align_to, Rendered):
            back = self.align_to.mol
            return back if isinstance(back, Mol) else mol(self.align_to.source)
        return self.align_to

    def align_wire(self) -> AlignTo | None:
        if self.align_to is None:
            return None
        if self.atom_map is not None:
            ref = self.align_to
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
                atom_map=[(int(q), int(r)) for q, r in self.atom_map],
            )
        if isinstance(self.align_to, Rendered):
            back = self.align_to.mol
            if isinstance(back, Mol) and back.id:
                return str(back.id)
            mid = self.align_to.molecule.id
            return str(mid) if mid else "m_0"
        return self.align_to.id or "m_0"


def mol(
    smiles_or_molfile: str,
    *,
    id: str | None = None,
    align_to: str | None = None,
    name: str | None = None,
    label: str | None = None,
    label_pos: LabelPos | None = None,
    label_color: str | None = None,
    label_scale: float | None = None,
) -> Mol:
    """Construct a ``Mol`` (JS / Rust ``xpict.mol``).

    ``name`` (preferred) / ``label`` (alias) set wire ``MolNode.name`` — painted
    inside the mol SVG by Rust. Default side is below; optional ``label_pos`` /
    color / scale map onto styled ``MolText``.
    """
    text = smiles_or_molfile.strip()
    if not text:
        raise ValueError("mol() requires a non-empty SMILES or molfile")
    if name is not None and label is not None and name != label:
        raise ValueError("mol() got both name= and label=; use only one")
    caption = name if name is not None else label
    return Mol(
        source=text,
        id=id,
        align_to=align_to,
        label=caption,
        label_pos=label_pos,
        label_color=label_color,
        label_scale=label_scale,
    )


def render_mol(input: Mol | str, opts: MolRenderOptions | None = None) -> Rendered:
    """Build a DepictSpec and run Rust ``depict`` → ``Rendered``."""
    from xpict.depict_spec import depict

    m = mol(input) if isinstance(input, str) else input
    options = opts if opts is not None else MolRenderOptions()
    if options.atom_map is not None and options.align_to is None:
        raise ValueError("atom_map requires align_to")

    rows = depict(m.to_spec(options))
    if not rows:
        raise RuntimeError("depict returned no molecules")
    painted = rows[-1]
    painted.mol = m
    if m.frame_molblock is None and options.align_to is None:
        m.frame_molblock = painted.frame_molblock
    return painted


# Public alias used by ``xpict.render(mol, …)``.
render = render_mol
