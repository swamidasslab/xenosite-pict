"""Indigo layout backend (optional alternate to native)."""

from __future__ import annotations

from xpict.backends.base import register, warn_unsupported
from xpict.contracts.layout import AtomLayout, BondLayout, MoleculeLayout
from xpict.contracts.spec import MoleculeSpec
from xpict.structure import cx_atom_labels, structure_smiles


def _safe_call(fn, default=None):
    try:
        return fn()
    except Exception:
        return default


def _element_label(
    element: str,
    hcount: int,
    *,
    charge: int,
    radical: int,
    isotope: int | None,
) -> str | None:
    """Indigo/RDKit terminal-hetero label: ``OH``, ``NH2``, ``NH4``; hide plain C.

    Implicit H comes from Indigo ``countImplicitHydrogens`` (same source the
    Indigo renderer uses for ``OH`` / ``NH2`` glyphs).
    """
    if element == "C" and charge == 0 and radical == 0 and not isotope:
        return None
    label = element
    if hcount == 1:
        label += "H"
    elif hcount > 1:
        label += f"H{hcount}"
    return label


def _is_star_symbol(el: str, *, is_pseudo: bool, is_rsite: bool) -> bool:
    if is_pseudo or is_rsite:
        return True
    if el in {"*", "R"}:
        return True
    if el.startswith("_"):
        return True
    # CX aliases folded into the symbol: R1, R12, Ap, etc.
    if el.startswith("R") and el[1:].isdigit():
        return True
    return False


@register("indigo")
class IndigoBackend:
    name = "indigo"

    def layout(self, mol: MoleculeSpec) -> MoleculeLayout:
        try:
            from indigo import Indigo
        except ImportError as e:
            raise ImportError(
                "Indigo backend requires epam.indigo. "
                "Install with: pip install 'xpict[indigo]'"
            ) from e

        indigo = Indigo()
        if mol.esmiles:
            warn_unsupported(self.name, "esmiles", "Using SMILES before <sep> only for now.")
        cx_labels: list[str | None] = []
        if mol.cxsmiles:
            imol = indigo.loadMolecule(mol.cxsmiles)
            cx_labels = cx_atom_labels(mol.cxsmiles)
        elif mol.molfile:
            imol = indigo.loadMolecule(mol.molfile)
        else:
            smiles = structure_smiles(mol)
            if not smiles:
                raise ValueError(
                    "indigo backend requires smiles, cxsmiles, esmiles, or molfile"
                )
            imol = indigo.loadMolecule(smiles)
            cx_labels = cx_atom_labels(smiles)

        imol.layout()
        try:
            imol.dearomatize()
        except Exception:
            pass

        atoms: list[AtomLayout] = []
        for atom in imol.iterateAtoms():
            x, y, _z = atom.xyz()
            el = atom.symbol()
            charge = int(atom.charge())
            isotope_raw = int(atom.isotope())
            isotope = isotope_raw if isotope_raw else None
            is_pseudo = bool(_safe_call(atom.isPseudoatom, False))
            is_rsite = bool(_safe_call(atom.isRSite, False))
            is_star = _is_star_symbol(el, is_pseudo=is_pseudo, is_rsite=is_rsite)

            radical = 0
            if not is_star:
                radical = int(_safe_call(atom.radicalElectrons, 0) or 0)
                # Indigo reports radicalElectrons=1 on closed-shell anions ([O-] → 1).
                if charge < 0 and radical:
                    radical = 0
            hcount = 0
            if not is_star:
                hcount = int(_safe_call(atom.countImplicitHydrogens, 0) or 0)

            idx = atom.index()
            cx = cx_labels[idx] if idx < len(cx_labels) else None
            name = _safe_call(atom.name, None)

            if is_star:
                # element is always '*' for attachment points; label carries the name.
                label = cx or name
                if not label:
                    label = el if el not in {"*", "R"} else "*"
                element = "*"
            else:
                element = el
                if cx or name:
                    label = cx or name
                else:
                    label = _element_label(el, hcount, charge=charge, radical=radical, isotope=isotope)

            atoms.append(
                AtomLayout(
                    index=idx,
                    element=element,
                    x=float(x),
                    y=float(y),
                    charge=charge,
                    isotope=isotope,
                    radical=radical,
                    label=label,
                )
            )

        bonds: list[BondLayout] = []
        for bond in imol.iterateBonds():
            st = int(bond.bondStereo())
            stereo = "none"
            if st == indigo.UP:
                stereo = "up"
            elif st == indigo.DOWN:
                stereo = "down"
            elif st == indigo.EITHER:
                stereo = "either"
            bonds.append(
                BondLayout(
                    index=bond.index(),
                    begin=bond.source().index(),
                    end=bond.destination().index(),
                    order=float(bond.bondOrder()),
                    stereo=stereo,
                )
            )
        return MoleculeLayout(id=mol.id, atoms=atoms, bonds=bonds, backend=self.name)
