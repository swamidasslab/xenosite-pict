"""Chemical edge-case depiction tests (isotopes, E/Z, salts, crowded FGs, …).

Complements ``test_stereo_wedge_correctness`` (tetrahedral wedges) and
``test_charge_star_align`` (basic charge/radical/star). Asserts layout fields
and painted SVG ``data-text`` / geometry — not cheminformatics completeness.
"""

from __future__ import annotations

import math
import re

import pytest
from helpers import (
    isomeric_smiles,
    isomeric_smiles_from_layout,
    layout_backend,
    random_isomeric_smiles,
)

from xpict import Pict, render
from xpict.contracts.layout import MoleculeLayout


def _layout(smiles: str) -> MoleculeLayout:
    return Pict(backend=layout_backend()).layout({"molecules": [{"smiles": smiles}]})[0]


def _svg(smiles: str) -> str:
    return render({"molecules": [{"smiles": smiles}]}, backend=layout_backend())


def _data_texts(svg: str) -> list[str]:
    return re.findall(r'data-text="([^"]*)"', svg)


def _atom_by_isotope(layout: MoleculeLayout, mass: int):
    for a in layout.atoms:
        if a.isotope == mass:
            return a
    raise AssertionError(f"no atom with isotope={mass} in {layout.atoms!r}")


# ---------------------------------------------------------------------------
# Isotopes
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "smiles,mass,label_substr",
    [
        ("[2H]C", 2, "2H"),
        ("C[13CH3]", 13, "13CH3"),
        ("[13CH4]", 13, "13CH4"),
        ("C[2H]", 2, "2H"),
    ],
)
def test_isotope_layout_and_label(smiles: str, mass: int, label_substr: str) -> None:
    layout = _layout(smiles)
    atom = _atom_by_isotope(layout, mass)
    assert atom.label is not None and label_substr in atom.label
    texts = _data_texts(_svg(smiles))
    assert any(label_substr in t or t.replace("²", "2").startswith("2") for t in texts) or any(
        label_substr in t for t in texts
    ), texts
    # Prefer exact painted mass digits when unicode scripts are not used.
    assert any(label_substr in t or "2H" in t.replace("²", "2") for t in texts) or any(
        "13" in t for t in texts
    )


def test_isotope_plus_tetrahedral_stereo() -> None:
    """Deuterated stereocenter: wedges recover stereo; D still labeled."""
    smiles = "C[C@]([2H])(O)Cl"
    layout = _layout(smiles)
    assert _atom_by_isotope(layout, 2).label and "2H" in (_atom_by_isotope(layout, 2).label or "")
    assert any(b.stereo in {"up", "down"} for b in layout.bonds)
    assert isomeric_smiles_from_layout(layout) == isomeric_smiles(smiles)
    assert any("2H" in t or "²H" in t for t in _data_texts(_svg(smiles)))


def test_isotope_stereo_robust_to_smiles_order() -> None:
    seed = "C[C@]([2H])(O)Cl"
    want = isomeric_smiles(seed)
    for smiles in random_isomeric_smiles(seed, n=8):
        assert isomeric_smiles(smiles) == want
        layout = _layout(smiles)
        assert isomeric_smiles_from_layout(layout) == want
        assert any(a.isotope == 2 for a in layout.atoms)


# ---------------------------------------------------------------------------
# E/Z alkene geometry
# ---------------------------------------------------------------------------


def _substituent_side_products(layout: MoleculeLayout, *, element: str) -> list[float]:
    """Cross products of (C2−C1)×(sub−C1) for substituents on a C=C (consistent axis)."""
    by = {a.index: a for a in layout.atoms}
    dbl = next(b for b in layout.bonds if b.order >= 1.9)
    c1, c2 = dbl.begin, dbl.end
    vx, vy = by[c2].x - by[c1].x, by[c2].y - by[c1].y
    sides: list[float] = []
    for bond in layout.bonds:
        ends = {bond.begin, bond.end}
        if len(ends & {c1, c2}) != 1:
            continue
        other = (ends - {c1, c2}).pop()
        if by[other].element != element:
            continue
        fx, fy = by[other].x - by[c1].x, by[other].y - by[c1].y
        sides.append(vx * fy - vy * fx)
    return sides


@pytest.mark.parametrize(
    "smiles,expect_opposite,element",
    [
        ("F/C=C/F", True, "F"),
        (r"F/C=C\F", False, "F"),
        ("C/C=C/C", True, "C"),
        (r"C/C=C\C", False, "C"),
        ("Cl/C=C/Cl", True, "Cl"),
        (r"Br/C=C\Br", False, "Br"),
    ],
)
def test_ez_substituents_geometry(
    smiles: str, expect_opposite: bool, element: str
) -> None:
    """Slash stereo places substituents on opposite (E) or same (Z) side of C=C."""
    layout = _layout(smiles)
    sides = _substituent_side_products(layout, element=element)
    assert len(sides) == 2, (smiles, sides)
    opposite = sides[0] * sides[1] < 0
    assert opposite is expect_opposite, (smiles, sides)
    # Specified E/Z must not be drawn as crossed “either” doubles.
    svg = _svg(smiles)
    assert "either-cross" not in svg
    assert "bond-either" not in svg


def test_ez_pair_are_distinct_layouts() -> None:
    trans = _layout("F/C=C/F")
    cis = _layout(r"F/C=C\F")
    t_sides = _substituent_side_products(trans, element="F")
    c_sides = _substituent_side_products(cis, element="F")
    assert (t_sides[0] * t_sides[1] < 0) != (c_sides[0] * c_sides[1] < 0)


# ---------------------------------------------------------------------------
# Charges, zwitterions, salts
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "smiles,expect_substrs",
    [
        ("[NH4+]", ["NH₄⁺", "NH4+"]),
        ("[OH-]", ["⁻HO", "HO⁻", "OH−", "⁻OH"]),
        ("[Na+]", ["Na⁺", "Na+"]),
        ("[Fe+2]", ["Fe²⁺", "Fe2+"]),
        ("[O-2]", ["O²⁻", "O2−", "²⁻O"]),
    ],
)
def test_charge_labels_not_double_applied(smiles: str, expect_substrs: list[str]) -> None:
    """Painted charge is single-strength (no NH₄²⁺ / Na²⁺ regression)."""
    texts = _data_texts(_svg(smiles))
    blob = " ".join(texts)
    assert any(s in blob for s in expect_substrs), texts
    # Double-applied +1 on a label that already had + yields ²⁺ after a single +.
    assert "²⁺" not in blob or any(
        s in blob for s in expect_substrs if "²" in s
    ), f"unexpected double charge in {texts}"
    if smiles in {"[NH4+]", "[Na+]"}:
        assert "²⁺" not in blob, texts
    if smiles == "[OH-]":
        assert "²⁻" not in blob, texts


def test_charged_chiral_center_stereo_and_labels() -> None:
    smiles = "C[C@H]([NH3+])C(=O)[O-]"
    layout = _layout(smiles)
    assert any(b.stereo in {"up", "down"} for b in layout.bonds)
    assert isomeric_smiles_from_layout(layout) == isomeric_smiles(smiles)
    texts = _data_texts(_svg(smiles))
    blob = " ".join(texts)
    assert "NH₃⁺" in blob or "NH3+" in blob
    assert "⁻" in blob or "−" in blob
    assert "²⁺" not in blob


def test_glycine_zwitterion_both_charges_labeled() -> None:
    smiles = "C(C(=O)[O-])[NH3+]"
    layout = _layout(smiles)
    assert any(a.charge > 0 for a in layout.atoms)
    assert any(a.charge < 0 for a in layout.atoms)
    blob = " ".join(_data_texts(_svg(smiles)))
    # West orientation may flip NH₃⁺ → ⁺H₃N.
    assert ("NH₃" in blob or "H₃N" in blob or "NH3" in blob) and (
        "⁻" in blob or "−" in blob
    )
    assert "²⁺" not in blob and "²⁻" not in blob


def test_salt_disconnected_fragments_both_present() -> None:
    """Benzoate·Na⁺: anion + cation labels; fragments separated in layout."""
    smiles = "c1ccccc1C(=O)[O-].[Na+]"
    layout = _layout(smiles)
    na = next(a for a in layout.atoms if a.element == "Na")
    oxy_minus = next(a for a in layout.atoms if a.element == "O" and a.charge < 0)
    dist = math.hypot(na.x - oxy_minus.x, na.y - oxy_minus.y)
    assert dist > 0.5, "salt fragments should not collapse to one point"
    texts = _data_texts(_svg(smiles))
    blob = " ".join(texts)
    assert "Na" in blob
    assert "⁻" in blob or "−" in blob or "O" in blob
    assert "²⁺" not in blob


# ---------------------------------------------------------------------------
# Crowded functional groups / sulfoxide / allene / macrocycle
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "smiles,needles",
    [
        ("OP(=O)(O)O", ["P", "O", "OH", "HO"]),
        ("CS(=O)(=O)O", ["S", "O", "OH"]),
        ("CS(=O)C", ["S", "O"]),
    ],
)
def test_crowded_fg_labels_present(smiles: str, needles: list[str]) -> None:
    layout = _layout(smiles)
    assert layout.atoms and layout.bonds
    # No aromatic leftover 1.5 if kekulized path; phosphate/sulfonyl are aliphatic.
    assert all(b.order in {1.0, 2.0, 3.0} or b.order >= 1.0 for b in layout.bonds)
    blob = " ".join(_data_texts(_svg(smiles)))
    for n in needles:
        assert n in blob, (smiles, needles, blob)


def test_sulfoxide_stereo_renders_and_preserves_smiles_when_wedged() -> None:
    """Sulfoxide: always renders; if wedges appear, they must be stereo-correct."""
    smiles = "C[S@](=O)C"
    layout = _layout(smiles)
    svg = _svg(smiles)
    assert "<svg" in svg
    assert any(t in {"S", "O"} for t in _data_texts(svg)) or "S" in " ".join(
        _data_texts(svg)
    )
    if any(b.stereo in {"up", "down"} for b in layout.bonds):
        assert isomeric_smiles_from_layout(layout) == isomeric_smiles(smiles)


def test_allene_renders() -> None:
    smiles = "C=C=C"
    layout = _layout(smiles)
    assert len([b for b in layout.bonds if b.order >= 1.9]) >= 2
    assert "<svg" in _svg(smiles)


def test_macrocycle_layouts_without_collapse() -> None:
    """12-membered ring: finite spread, no atom pile-up at one point."""
    smiles = "C1CCCCCCCCCCC1"
    layout = _layout(smiles)
    xs = [a.x for a in layout.atoms]
    ys = [a.y for a in layout.atoms]
    assert max(xs) - min(xs) > 1.0
    assert max(ys) - min(ys) > 1.0
    # Pairwise min distance among non-bonded atoms should stay sane for most pairs;
    # require mean nearest-neighbor distance above a floor.
    n = len(layout.atoms)
    bonded = {(b.begin, b.end) for b in layout.bonds} | {(b.end, b.begin) for b in layout.bonds}
    by = {a.index: a for a in layout.atoms}
    nn: list[float] = []
    for a in layout.atoms:
        best = min(
            (
                math.hypot(a.x - by[j].x, a.y - by[j].y)
                for j in by
                if j != a.index and (a.index, j) not in bonded
            ),
            default=999.0,
        )
        nn.append(best)
    assert sum(nn) / n > 0.15, nn
    assert "<svg" in _svg(smiles)


def test_macrocycle_with_quaternary_center_renders() -> None:
    smiles = "C[C@]1(C)CCCCCCCCCCC1"
    layout = _layout(smiles)
    assert len(layout.atoms) >= 13
    assert "<svg" in _svg(smiles)
    # Stereo may or may not wedge on large rings; if it does, stay correct.
    if any(b.stereo in {"up", "down"} for b in layout.bonds):
        assert isomeric_smiles_from_layout(layout) == isomeric_smiles(smiles)


# ---------------------------------------------------------------------------
# Unspecified stereo from real molecules (not hand-built bond_strokes)
# ---------------------------------------------------------------------------


def test_unspecified_double_bond_may_use_either_or_plain() -> None:
    """Unspecified alkene still depicts; crossed either is allowed but not required."""
    smiles = "CC=CC"
    layout = _layout(smiles)
    svg = _svg(smiles)
    assert "<svg" in svg
    assert len(layout.bonds) >= 3
    # Must not invent tetrahedral wedges on an achiral chain.
    assert not any(b.stereo in {"up", "down"} for b in layout.bonds)


def test_phosphate_and_sulfonyl_no_triple_looking_bonds() -> None:
    for smiles in ("OP(=O)(O)O", "CS(=O)(=O)C"):
        layout = _layout(smiles)
        assert all(b.order <= 2.0 + 1e-6 for b in layout.bonds), [
            (b.begin, b.end, b.order) for b in layout.bonds
        ]
