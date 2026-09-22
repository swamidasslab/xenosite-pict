"""Python client: objects/methods over the same Rust ``depict_molecule`` surface as JS.

JS keeps a flat ``xpict`` namespace of plain objects. Python prefers
``Mol.from_source(...).render().to_svg()`` (helpers still exist for parity).
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from typing import Any, Sequence

from xpict import _native
from xpict.contracts.scene import Scene
from xpict.draw.metrics import BOND_PX
from xpict.draw.svg import scene_to_svg
from xpict.structure import cx_atom_labels

SCALE = BOND_PX  # 20 — same as JS ``SCALE``


@dataclass
class Mol:
    """Input molecule — SMILES/CXSMILES/molfile plus optional alignment frame.

    Python prefers methods (``Mol.from_source(...).render()``). JS keeps a flat
    ``xpict`` namespace of plain objects / functions.
    """

    source: str
    frame_molblock: str | None = None

    @classmethod
    def from_source(cls, smiles_or_molfile: str) -> Mol:
        text = smiles_or_molfile.strip()
        if not text:
            raise ValueError("Mol requires a non-empty SMILES or molfile")
        return cls(source=text)

    def layout(
        self,
        *,
        id: str | None = None,
        template_molblock: str | None = None,
    ) -> tuple[dict[str, Any], str]:
        """RDKit/native layout → ``MoleculeIn`` dict + pose molblock."""
        laid, pose = layout_molecule(
            self.source,
            id=id,
            template_molblock=template_molblock or self.frame_molblock,
        )
        if pose and not self.frame_molblock:
            self.frame_molblock = pose
        return laid, pose

    def render(
        self,
        opts: MolRenderOptions | dict[str, Any] | None = None,
        /,
        **kwargs: Any,
    ) -> Rendered:
        """Layout → Rust ``depict_molecule`` → :class:`Rendered`."""
        if opts is None and kwargs:
            opts = MolRenderOptions(**kwargs)
        elif isinstance(opts, dict):
            merged = {**opts, **kwargs} if kwargs else opts
            opts = MolRenderOptions(**merged)
        elif opts is not None and kwargs:
            raise TypeError("pass options via opts= or kwargs, not both")
        return render(self, opts)


@dataclass
class Rendered:
    """Painted depiction — editable ``scene`` plus alignment frame."""

    width: float
    height: float
    scene: Scene
    molecule: dict[str, Any]
    source: str
    frame_molblock: str
    coords: list[dict[str, Any]] = field(default_factory=list)
    svg_coords: list[dict[str, Any]] = field(default_factory=list)
    bonds: list[dict[str, Any]] = field(default_factory=list)
    mol: Mol | None = None

    def to_svg(self) -> str:
        """Serialize ``self.scene`` to an SVG string."""
        return scene_to_svg(self.scene)

    def render_aligned(self, other: Mol | str, **kwargs: Any) -> Rendered:
        """Render ``other`` aligned onto this pose (``align_to=self``)."""
        target = other if isinstance(other, Mol) else Mol.from_source(other)
        return target.render(align_to=self, **kwargs)


@dataclass
class MolRenderOptions:
    id: str | None = None
    color: str | None = None
    atom_shade: list[float] | None = None
    bond_shade: list[float] | None = None
    mark_atoms: list[int] | None = None
    mark_bonds: list[tuple[int, int]] | None = None
    star_labels: list[str | None] | None = None
    bold_labels: bool | None = None
    align_to: Mol | Rendered | None = None


def mol(smiles_or_molfile: str) -> Mol:
    """Construct a :class:`Mol` (JS-shaped helper; prefer ``Mol.from_source``)."""
    return Mol.from_source(smiles_or_molfile)


def to_svg(scene: Scene | dict[str, Any] | Rendered) -> str:
    """Scene / :class:`Rendered` → SVG (JS ``xpict.toSvg``)."""
    if isinstance(scene, Rendered):
        return scene.to_svg()
    if isinstance(scene, dict):
        scene = Scene.model_validate(scene)
    return scene_to_svg(scene)


def _is_star(atom: dict[str, Any]) -> bool:
    if atom.get("element") == "*":
        return True
    return atom.get("z") == 0


def _apply_star_labels(
    molecule: dict[str, Any], labels: Sequence[str | None] | None
) -> dict[str, Any]:
    if not labels:
        return molecule
    stars = [i for i, a in enumerate(molecule["atoms"]) if _is_star(a)]
    if not stars:
        return molecule
    atoms = [dict(a) for a in molecule["atoms"]]
    for k, raw in enumerate(labels):
        if k >= len(stars):
            break
        label = "*" if raw is None or str(raw).strip() == "" else str(raw).strip()
        atoms[stars[k]] = {**atoms[stars[k]], "label": label}
    return {**molecule, "atoms": atoms}


def _apply_cx_by_index(molecule: dict[str, Any], source: str) -> dict[str, Any]:
    aliases = cx_atom_labels(source)
    if not aliases:
        return molecule
    atoms = []
    for a in molecule["atoms"]:
        idx = int(a["index"])
        raw = aliases[idx] if idx < len(aliases) else None
        if raw:
            atoms.append({**a, "label": raw})
        else:
            atoms.append(a)
    return {**molecule, "atoms": atoms}


def _apply_opts(
    molecule: dict[str, Any], opts: MolRenderOptions, source: str
) -> dict[str, Any]:
    out = {
        **molecule,
        "atoms": [dict(a) for a in molecule["atoms"]],
        "bonds": [dict(b) for b in molecule["bonds"]],
    }
    if opts.id is not None:
        out["id"] = opts.id
    if opts.color is not None:
        out["color"] = opts.color
    if opts.atom_shade is not None:
        out["atom_shade"] = list(opts.atom_shade)
    if opts.bond_shade is not None:
        out["bond_shade"] = list(opts.bond_shade)
    if opts.mark_atoms is not None:
        out["mark_atoms"] = list(opts.mark_atoms)
    if opts.mark_bonds is not None:
        out["mark_bonds"] = [list(p) for p in opts.mark_bonds]
    if opts.bold_labels is not None:
        out["bold_labels"] = bool(opts.bold_labels)
    if opts.star_labels is not None:
        out = _apply_star_labels(out, opts.star_labels)
    else:
        out = _apply_cx_by_index(out, source)
    return out


def _atom_label(element: str, imp_hs: int, charge: int) -> str | None:
    """Match JS ``atomLabel`` in rdkit-layout.ts."""
    if element == "C" and not charge:
        return None
    if element == "*":
        return "*"
    text = element
    if imp_hs == 1:
        text = f"{element}H"
    elif imp_hs > 1:
        text = f"{element}H{imp_hs}"
    if charge:
        sign = "+" if charge > 0 else "−"
        mag = abs(charge)
        text = f"{text}{sign}" if mag == 1 else f"{text}{mag}{sign}"
    return text


def _layout_rdkit(
    source: str,
    *,
    id: str | None = None,
    template_molblock: str | None = None,
) -> tuple[dict[str, Any], str]:
    """RDKit layout → MoleculeIn dict + pose molblock (JS ``layoutWithRdkit``)."""
    from rdkit import Chem
    from rdkit.Chem import rdDepictor

    if "V2000" in source or "V3000" in source:
        rmol = Chem.MolFromMolBlock(source, sanitize=True, removeHs=False)
    else:
        rmol = Chem.MolFromSmiles(source)
    if rmol is None:
        raise ValueError(f"RDKit could not parse molecule: {source[:80]!r}")

    try:
        Chem.Kekulize(rmol, clearAromaticFlags=True)
    except Exception:
        pass

    if template_molblock:
        tmpl = Chem.MolFromMolBlock(template_molblock, sanitize=True, removeHs=False)
        if tmpl is not None and tmpl.GetNumConformers():
            try:
                rdDepictor.GenerateDepictionMatching2DStructure(rmol, tmpl)
            except Exception:
                rdDepictor.Compute2DCoords(rmol)
        else:
            rdDepictor.Compute2DCoords(rmol)
    elif rmol.GetNumConformers() == 0:
        rdDepictor.Compute2DCoords(rmol)

    try:
        from rdkit.Chem.rdmolops import WedgeMolBonds

        WedgeMolBonds(rmol, rmol.GetConformer())
    except Exception:
        pass

    conf = rmol.GetConformer()
    coords = [
        (float(conf.GetAtomPosition(i).x), float(conf.GetAtomPosition(i).y))
        for i in range(rmol.GetNumAtoms())
    ]
    bonds_rd = list(rmol.GetBonds())
    lengths = []
    for b in bonds_rd:
        i, j = b.GetBeginAtomIdx(), b.GetEndAtomIdx()
        x1, y1 = coords[i]
        x2, y2 = coords[j]
        lengths.append(((x2 - x1) ** 2 + (y2 - y1) ** 2) ** 0.5)
    mean = (sum(lengths) / len(lengths)) if lengths else 1.5
    if mean < 1e-6:
        mean = 1.5
    scale = SCALE / mean
    max_y = max((y for _, y in coords), default=0.0)

    atoms: list[dict[str, Any]] = []
    for i, atom in enumerate(rmol.GetAtoms()):
        z = int(atom.GetAtomicNum())
        element = "*" if z == 0 else atom.GetSymbol()
        charge = int(atom.GetFormalCharge())
        imp_hs = int(atom.GetTotalNumHs())
        x0, y0 = coords[i]
        x = x0 * scale
        y = (max_y - y0) * scale
        label = _atom_label(element, imp_hs, charge)
        entry: dict[str, Any] = {"index": i, "z": z, "x": x, "y": y}
        if charge:
            entry["charge"] = charge
        if label:
            entry["label"] = label
        atoms.append(entry)

    bonds: list[dict[str, Any]] = []
    for bi, b in enumerate(bonds_rd):
        order = float(b.GetBondTypeAsDouble())
        stereo = None
        if b.GetBondDir() == Chem.BondDir.BEGINWEDGE:
            stereo = "up"
        elif b.GetBondDir() == Chem.BondDir.BEGINDASH:
            stereo = "down"
        elif (
            b.GetBondDir() == Chem.BondDir.UNKNOWN
            or b.GetStereo() == Chem.BondStereo.STEREOANY
        ):
            stereo = "either"
        bd: dict[str, Any] = {
            "index": bi,
            "begin": b.GetBeginAtomIdx(),
            "end": b.GetEndAtomIdx(),
            "order": order,
        }
        if stereo:
            bd["stereo"] = stereo
        bonds.append(bd)

    molecule: dict[str, Any] = {"atoms": atoms, "bonds": bonds}
    if id is not None:
        molecule["id"] = id
    return molecule, Chem.MolToMolBlock(rmol)


def _layout_native(source: str, *, id: str | None = None) -> tuple[dict[str, Any], str]:
    """Fallback when RDKit is unavailable — native SMILES layout → MoleculeIn."""
    from xpict.backends.native_layout import layout_smiles

    smiles = source.split("|", 1)[0].strip()
    lay = layout_smiles(smiles, mol_id=id)
    coords = [(a.x, a.y) for a in lay.atoms]
    lengths = []
    idx_of = {a.index: i for i, a in enumerate(lay.atoms)}
    for b in lay.bonds:
        ia, ib = idx_of[b.begin], idx_of[b.end]
        x1, y1 = coords[ia]
        x2, y2 = coords[ib]
        lengths.append(((x2 - x1) ** 2 + (y2 - y1) ** 2) ** 0.5)
    mean = (sum(lengths) / len(lengths)) if lengths else 1.5
    scale = SCALE / mean if mean > 1e-6 else SCALE
    max_y = max((y for _, y in coords), default=0.0)
    aliases = cx_atom_labels(source)
    atoms = []
    for a in lay.atoms:
        x = a.x * scale
        y = (max_y - a.y) * scale
        label = a.label
        if a.index < len(aliases) and aliases[a.index]:
            label = aliases[a.index]
        entry: dict[str, Any] = {
            "index": a.index,
            "element": a.element,
            "x": x,
            "y": y,
        }
        if a.charge:
            entry["charge"] = a.charge
        if label:
            entry["label"] = label
        atoms.append(entry)
    bonds = [
        {
            "index": b.index,
            "begin": b.begin,
            "end": b.end,
            "order": b.order,
            **({"stereo": b.stereo} if b.stereo and b.stereo != "none" else {}),
        }
        for b in lay.bonds
    ]
    molecule: dict[str, Any] = {"atoms": atoms, "bonds": bonds}
    if id is not None:
        molecule["id"] = id
    return molecule, ""


def layout_molecule(
    source: str,
    *,
    id: str | None = None,
    template_molblock: str | None = None,
) -> tuple[dict[str, Any], str]:
    try:
        import rdkit  # noqa: F401

        return _layout_rdkit(source, id=id, template_molblock=template_molblock)
    except ImportError:
        return _layout_native(source, id=id)


def render(
    input: Mol | str,
    opts: MolRenderOptions | dict[str, Any] | None = None,
) -> Rendered:
    """JS ``xpict.render`` — layout → Rust depict → Scene."""
    if isinstance(opts, dict):
        opts = MolRenderOptions(**opts)
    opts = opts or MolRenderOptions()
    m = mol(input) if isinstance(input, str) else input

    template = None
    if opts.align_to is not None:
        target = opts.align_to
        if isinstance(target, Rendered):
            template = target.frame_molblock
        else:
            if not target.frame_molblock:
                _, mb = layout_molecule(target.source)
                target.frame_molblock = mb
            template = target.frame_molblock

    laid, pose_mb = layout_molecule(
        m.source, id=opts.id, template_molblock=template
    )
    if not m.frame_molblock:
        m.frame_molblock = pose_mb

    molecule = _apply_opts(laid, opts, m.source)
    scene_json = _native.depict_molecule(json.dumps(molecule))
    scene = Scene.model_validate_json(scene_json)

    coords = [
        {
            "index": a["index"],
            "element": a.get("element") or ("*" if a.get("z") == 0 else "C"),
            "x": a["x"],
            "y": a["y"],
            **({"label": a["label"]} if a.get("label") else {}),
            **({"charge": a["charge"]} if a.get("charge") else {}),
        }
        for a in molecule["atoms"]
    ]
    return Rendered(
        width=scene.width,
        height=scene.height,
        scene=scene,
        molecule=molecule,
        source=m.source,
        frame_molblock=pose_mb or m.frame_molblock or "",
        coords=coords,
        svg_coords=coords,
        bonds=[
            {"begin": b["begin"], "end": b["end"], "order": b["order"]}
            for b in molecule["bonds"]
        ],
        mol=m,
    )


class Xpict:
    """Python client entrypoint — methods wrap :class:`Mol` / :class:`Rendered`.

    Prefer ``Mol.from_source(...).render().to_svg()``. The ``xpict`` instance
    mirrors the JS namespace for cross-language examples.
    """

    def mol(self, smiles_or_molfile: str) -> Mol:
        return Mol.from_source(smiles_or_molfile)

    def render(
        self,
        input: Mol | str,
        opts: MolRenderOptions | dict[str, Any] | None = None,
        /,
        **kwargs: Any,
    ) -> Rendered:
        if isinstance(input, str):
            return Mol.from_source(input).render(opts, **kwargs)
        return input.render(opts, **kwargs)

    def to_svg(self, scene: Scene | dict[str, Any] | Rendered) -> str:
        return to_svg(scene)

    # JS camelCase alias
    toSvg = to_svg


xpict = Xpict()

__all__ = [
    "Mol",
    "MolRenderOptions",
    "Rendered",
    "Xpict",
    "layout_molecule",
    "mol",
    "render",
    "to_svg",
    "xpict",
]
