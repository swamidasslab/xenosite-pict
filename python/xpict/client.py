"""Single-molecule client — parity with JS ``xpict.mol`` / ``render`` / ``toSvg``.

All three languages ship the same surface:

    mol(source) → render(mol, opts?) → Rendered → to_svg(scene)

``align_to`` accepts a prior ``Mol`` or ``Rendered`` (pose molblock under the
hood). Layout + MCS align use RDKit at the language edge; paint is
``xpict._native.depict_molecule``.
"""

from __future__ import annotations

import json
import math
from dataclasses import dataclass, fields
from typing import Any, Sequence

from xpict.contracts.scene import Scene
from xpict.draw.metrics import SCALE
from xpict.draw.svg import scene_to_svg as _scene_to_svg
from xpict.structure import cx_atom_labels

# Keep in sync with ``xpict::align_opts::MIN_MCS_ATOMS``.
_MIN_MCS_ATOMS = 3


@dataclass
class Mol:
    """Input molecule — SMILES / CXSMILES / molfile plus optional align frame."""

    source: str
    frame_molblock: str | None = None

    def render(self, opts: MolRenderOptions | None = None) -> Rendered:
        return render(self, opts)

    def ensure_frame(self) -> str:
        """Materialize (and cache) a coord-bearing molblock for ``align_to``."""
        if not self.frame_molblock:
            _mol_in, pose, _meta = _layout_with_rdkit(self.source, template=None, id=None)
            del _mol_in
            self.frame_molblock = pose
        return self.frame_molblock


@dataclass
class SvgAtom:
    index: int
    element: str
    x: float
    y: float
    label: str | None = None
    charge: int = 0


@dataclass
class SvgBond:
    index: int
    begin: int
    end: int
    order: float
    stereo: str | None = None


@dataclass
class Rendered:
    """Painted depiction — editable ``scene`` plus alignment frame."""

    width: float
    height: float
    scene: Scene
    molecule: dict[str, Any]
    source: str
    frame_molblock: str
    coords: list[SvgAtom]
    svg_coords: list[SvgAtom]
    bonds: list[SvgBond]
    mol: Mol | None = None

    def to_svg(self) -> str:
        return to_svg(self.scene)

    def frame(self) -> str:
        """Pose molblock for ``align_to``."""
        return self.frame_molblock


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
    align_to: Mol | Rendered | None = None
    #: Pairs ``(query_atom, template_atom)``. Requires ``align_to``; skips MCS.
    atom_map: list[tuple[int, int]] | None = None


AlignTarget = Mol | Rendered


def mol(smiles_or_molfile: str) -> Mol:
    """Construct a ``Mol`` (JS / Rust ``xpict.mol``)."""
    text = smiles_or_molfile.strip()
    if not text:
        raise ValueError("mol() requires a non-empty SMILES or molfile")
    return Mol(source=text)


def to_svg(scene: Scene | dict[str, Any]) -> str:
    """Scene JSON / ``Scene`` → SVG string (JS ``xpict.toSvg``)."""
    if isinstance(scene, dict):
        scene = Scene.model_validate(scene)
    return _scene_to_svg(scene)


def render(
    input: Mol | str,
    opts: MolRenderOptions | dict[str, Any] | None = None,
) -> Rendered:
    """Layout → native ``depict_molecule`` → ``Rendered`` (JS ``xpict.render``)."""
    m = mol(input) if isinstance(input, str) else input
    options = _coerce_opts(opts)

    template: str | None = None
    if options.align_to is not None:
        template = _ensure_frame(options.align_to)
    elif options.atom_map is not None:
        raise ValueError("atom_map requires align_to")

    laid, pose_mb, _meta = _layout_with_rdkit(
        m.source,
        template=template,
        id=options.id,
        atom_map=options.atom_map,
    )
    if m.frame_molblock is None and template is None:
        m.frame_molblock = pose_mb

    molecule = _apply_opts(laid, options, m.source)
    from xpict import _native

    scene = Scene.model_validate(
        json.loads(_native.depict_molecule(json.dumps(molecule)))
    )
    coords = _to_coord_list(molecule["atoms"])
    bonds = [
        SvgBond(
            index=int(b["index"]),
            begin=int(b["begin"]),
            end=int(b["end"]),
            order=float(b["order"]),
            stereo=b.get("stereo"),
        )
        for b in molecule["bonds"]
    ]
    return Rendered(
        width=float(scene.width),
        height=float(scene.height),
        scene=scene,
        molecule=molecule,
        source=m.source,
        frame_molblock=pose_mb,
        coords=coords,
        svg_coords=list(coords),
        bonds=bonds,
        mol=m,
    )


def _coerce_opts(
    opts: MolRenderOptions | dict[str, Any] | None,
) -> MolRenderOptions:
    if opts is None:
        return MolRenderOptions()
    if isinstance(opts, MolRenderOptions):
        return opts
    known = {f.name for f in fields(MolRenderOptions)}
    return MolRenderOptions(**{k: v for k, v in opts.items() if k in known})


def _ensure_frame(target: AlignTarget) -> str:
    if isinstance(target, Rendered):
        if not target.frame_molblock:
            raise ValueError("Rendered is missing frame_molblock")
        return target.frame_molblock
    return target.ensure_frame()


def _sanitize_dummy_molblock(molblock: str) -> str:
    """Rewrite dummy ``R`` / drop ``M  ALS`` — parity with JS / Rust."""
    lines = [
        line.replace(" R ", " * ")
        for line in molblock.splitlines()
        if not line.startswith("M  ALS")
    ]
    return "\n".join(lines)


def _looks_like_molblock(source: str) -> bool:
    return "V2000" in source or "V3000" in source


def _smiles_base(source: str) -> str:
    """Strip CX ``|…|`` trailer for MolFromSmiles when needed."""
    pipe = source.find("|")
    if pipe < 0:
        return source.strip()
    return source[:pipe].strip()


def _element_label(element: str, imp_hs: int, charge: int) -> str | None:
    if element == "C" and charge == 0:
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


def _bond_order(bond) -> float:
    from rdkit import Chem

    t = bond.GetBondType()
    if t == Chem.BondType.TRIPLE:
        return 3.0
    if t == Chem.BondType.DOUBLE:
        return 2.0
    if t == Chem.BondType.AROMATIC:
        return 1.5
    return 1.0


def _bond_stereo(bond) -> str | None:
    from rdkit import Chem

    if bond.GetBondDir() == Chem.BondDir.BEGINWEDGE:
        return "up"
    if bond.GetBondDir() == Chem.BondDir.BEGINDASH:
        return "down"
    if bond.GetBondDir() == Chem.BondDir.UNKNOWN:
        return "either"
    if bond.GetStereo() == Chem.BondStereo.STEREOANY:
        return "either"
    return None


def _mean_bond_scale(coords: list[tuple[float, float]], bonds: list[tuple[int, int]]) -> float:
    lengths: list[float] = []
    for i, j in bonds:
        if i >= len(coords) or j >= len(coords):
            continue
        ax, ay = coords[i]
        bx, by = coords[j]
        lengths.append(math.hypot(bx - ax, by - ay))
    mean = sum(lengths) / len(lengths) if lengths else 1.5
    if mean < 1e-6:
        mean = 1.5
    return SCALE / mean


def _parse_rdkit(source: str):
    from rdkit import Chem

    text = source.strip()
    if _looks_like_molblock(text):
        rmol = Chem.MolFromMolBlock(text, sanitize=True, removeHs=False)
    else:
        # Prefer full CX when present so atom count matches aliases.
        rmol = Chem.MolFromSmiles(text)
        if rmol is None:  # pyright: ignore[reportUnnecessaryComparison]
            rmol = Chem.MolFromSmiles(_smiles_base(text))
    if rmol is None:  # pyright: ignore[reportUnnecessaryComparison]
        raise ValueError(f"RDKit could not parse molecule ({text[:80]})")
    try:
        Chem.Kekulize(rmol, clearAromaticFlags=True)
    except Exception:
        pass
    return rmol


def _mol_to_molecule_in(
    rmol,
    *,
    id: str | None,
    scale: float,
    flip_max_y: float,
    source: str,
) -> dict[str, Any]:
    from rdkit.Chem import rdmolops

    conf = rmol.GetConformer()
    try:
        rdmolops.WedgeMolBonds(rmol, conf)
    except Exception:
        pass
    conf = rmol.GetConformer()

    atoms: list[dict[str, Any]] = []
    for atom in rmol.GetAtoms():
        idx = int(atom.GetIdx())
        pos = conf.GetAtomPosition(idx)
        z = int(atom.GetAtomicNum())
        el = "*" if z == 0 or atom.GetSymbol() == "*" else atom.GetSymbol()
        charge = int(atom.GetFormalCharge())
        imp_hs = int(atom.GetTotalNumHs())
        label = _element_label(el, imp_hs, charge)
        atoms.append(
            {
                "index": idx,
                "element": el,
                "z": z,
                "x": float(pos.x) * scale,
                "y": (flip_max_y - float(pos.y)) * scale,
                "label": label,
                "charge": charge,
            }
        )

    bonds: list[dict[str, Any]] = []
    for bond in rmol.GetBonds():
        stereo = _bond_stereo(bond)
        entry: dict[str, Any] = {
            "index": int(bond.GetIdx()),
            "begin": int(bond.GetBeginAtomIdx()),
            "end": int(bond.GetEndAtomIdx()),
            "order": _bond_order(bond),
        }
        if stereo:
            entry["stereo"] = stereo
        bonds.append(entry)

    out: dict[str, Any] = {"atoms": atoms, "bonds": bonds}
    if id is not None:
        out["id"] = id
    # CX aliases applied later in _apply_opts when star_labels omitted.
    del source
    return out


def layout_with_rdkit(
    source: str,
    *,
    template: str | None = None,
    id: str | None = None,
    atom_map: list[tuple[int, int]] | None = None,
    min_atoms: int | None = None,
) -> tuple[dict[str, Any], str, dict[str, Any]]:
    """Public RDKit layout helper used by the EdgePlan processor."""
    return _layout_with_rdkit(
        source,
        template=template,
        id=id,
        atom_map=atom_map,
        min_atoms=min_atoms,
    )


def _layout_with_rdkit(
    source: str,
    *,
    template: str | None,
    id: str | None,
    atom_map: list[tuple[int, int]] | None = None,
    min_atoms: int | None = None,
) -> tuple[dict[str, Any], str, dict[str, Any]]:
    """RDKit layout (+ optional MCS / explicit atom-map align) → MoleculeIn + pose + meta.

    Meta is ``{"method": "free"|"atom_map"|"mcs"|"none", "used_map": ...}``.
    """
    try:
        from rdkit import Chem
        from rdkit.Chem import rdDepictor, rdFMCS
    except ImportError as e:
        raise ImportError(
            "Single-mol render requires rdkit. Install with: pip install 'xpict[rdkit]'"
        ) from e

    floor = _MIN_MCS_ATOMS if min_atoms is None else int(min_atoms)
    rmol = _parse_rdkit(source)
    method = "free"
    used_map: list[tuple[int, int]] | None = None

    if template:
        tmpl = Chem.MolFromMolBlock(
            _sanitize_dummy_molblock(template), sanitize=True, removeHs=False
        )
        if tmpl is None:  # pyright: ignore[reportUnnecessaryComparison]
            raise ValueError("align_to template molblock could not be parsed")
        # Never mutate the caller's template pose.
        ref_pose = Chem.Mol(tmpl)
        if not ref_pose.GetNumConformers():
            rdDepictor.Compute2DCoords(ref_pose)

        aligned_ok = False
        try:
            map_for_depict: list[tuple[int, int]] | None = None
            if atom_map is not None:
                # Public pairs are (query, template); Depictor wants (template, query).
                if len(atom_map) >= floor:
                    map_for_depict = [(t, q) for q, t in atom_map]
                    used_map = list(atom_map)
                    method = "atom_map"
            else:
                from xpict.align_rdkit import mcs_params

                mcs = rdFMCS.FindMCS([ref_pose, rmol], mcs_params())
                if (
                    not getattr(mcs, "canceled", False)
                    and mcs.numAtoms >= floor
                ):
                    pattern = Chem.MolFromSmarts(mcs.smartsString)
                    if pattern is not None:  # pyright: ignore[reportUnnecessaryComparison]
                        ref_match = ref_pose.GetSubstructMatch(pattern)
                        mol_match = rmol.GetSubstructMatch(pattern)
                        if len(ref_match) >= floor and len(ref_match) == len(mol_match):
                            map_for_depict = list(
                                zip(ref_match, mol_match, strict=True)
                            )
                            used_map = list(
                                zip(mol_match, ref_match, strict=True)
                            )
                            method = "mcs"

            if map_for_depict is not None:
                params = rdDepictor.ConstrainedDepictionParams()
                params.allowRGroups = True
                params.acceptFailure = False
                rdDepictor.GenerateDepictionMatching2DStructure(
                    rmol, ref_pose, map_for_depict, -1, params
                )
                aligned_ok = True
        except Exception:
            aligned_ok = False
            method = "none"
            used_map = None

        if not aligned_ok:
            method = "none"
            used_map = None
            if not rmol.GetNumConformers():
                rdDepictor.Compute2DCoords(rmol)
            coords = [
                (
                    float(rmol.GetConformer().GetAtomPosition(i).x),
                    float(rmol.GetConformer().GetAtomPosition(i).y),
                )
                for i in range(rmol.GetNumAtoms())
            ]
            bond_pairs = [
                (int(b.GetBeginAtomIdx()), int(b.GetEndAtomIdx()))
                for b in rmol.GetBonds()
            ]
            scale = _mean_bond_scale(coords, bond_pairs)
            flip_max_y = max((y for _, y in coords), default=0.0)
        else:
            t_coords = [
                (
                    float(ref_pose.GetConformer().GetAtomPosition(i).x),
                    float(ref_pose.GetConformer().GetAtomPosition(i).y),
                )
                for i in range(ref_pose.GetNumAtoms())
            ]
            t_bonds = [
                (int(b.GetBeginAtomIdx()), int(b.GetEndAtomIdx()))
                for b in ref_pose.GetBonds()
            ]
            scale = _mean_bond_scale(t_coords, t_bonds)
            flip_max_y = max((y for _, y in t_coords), default=0.0)
    else:
        if not rmol.GetNumConformers():
            rdDepictor.Compute2DCoords(rmol)
        coords = [
            (
                float(rmol.GetConformer().GetAtomPosition(i).x),
                float(rmol.GetConformer().GetAtomPosition(i).y),
            )
            for i in range(rmol.GetNumAtoms())
        ]
        bond_pairs = [
            (int(b.GetBeginAtomIdx()), int(b.GetEndAtomIdx()))
            for b in rmol.GetBonds()
        ]
        scale = _mean_bond_scale(coords, bond_pairs)
        flip_max_y = max((y for _, y in coords), default=0.0)

    molecule = _mol_to_molecule_in(
        rmol, id=id, scale=scale, flip_max_y=flip_max_y, source=source
    )
    pose = _sanitize_dummy_molblock(Chem.MolToMolBlock(rmol))
    return molecule, pose, {"method": method, "used_map": used_map}


def _is_star(atom: dict[str, Any]) -> bool:
    if atom.get("element") == "*":
        return True
    return atom.get("z") == 0


def _apply_star_labels(
    molecule: dict[str, Any], labels: Sequence[str | None]
) -> dict[str, Any]:
    if not labels:
        return molecule
    atoms = [dict(a) for a in molecule["atoms"]]
    stars = [i for i, a in enumerate(atoms) if _is_star(a)]
    for k, raw in enumerate(labels):
        if k >= len(stars):
            break
        if raw is None or str(raw).strip() == "":
            label = "*"
        else:
            label = str(raw).strip()
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
        if raw is None or not str(raw).strip():
            atoms.append(a)
            continue
        atoms.append({**a, "label": str(raw).strip()})
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
    if opts.weight is not None:
        out["weight"] = float(opts.weight)
    if opts.scale is not None:
        out["scale"] = float(opts.scale)
    if opts.star_labels is not None:
        out = _apply_star_labels(out, opts.star_labels)
    else:
        out = _apply_cx_by_index(out, source)
    return out


def _to_coord_list(atoms: list[dict[str, Any]]) -> list[SvgAtom]:
    out: list[SvgAtom] = []
    for a in atoms:
        el = a.get("element") or "*"
        out.append(
            SvgAtom(
                index=int(a["index"]),
                element=str(el),
                x=float(a["x"]),
                y=float(a["y"]),
                label=a.get("label"),
                charge=int(a.get("charge") or 0),
            )
        )
    return out
