"""Minimal organic SMILES → topology for the native layout lab.

Enough for derisk gallery molecules (alkanes, aromatics, fused/bridged rings,
common heteroatoms). Not a full OpenSMILES implementation — Chematic/Indigo
remain the production parsers; this exists so native *layout* algorithms can
be exercised without a chem engine.
"""

from __future__ import annotations

from dataclasses import dataclass, field
import re


_ORGANIC = {
    "B": ("B", False),
    "C": ("C", False),
    "N": ("N", False),
    "O": ("O", False),
    "P": ("P", False),
    "S": ("S", False),
    "F": ("F", False),
    "Cl": ("Cl", False),
    "Br": ("Br", False),
    "I": ("I", False),
    "b": ("B", True),
    "c": ("C", True),
    "n": ("N", True),
    "o": ("O", True),
    "p": ("P", True),
    "s": ("S", True),
}

_BRACKET = re.compile(
    r"\["
    r"(?P<iso>\d+)?"
    r"(?P<el>[A-Z][a-z]?|[a-z])"
    r"(?P<arom_h>)?"  # placeholder kept for readability
    r"(?:@+|@@)?"
    r"(?:H(?P<h>\d?))?"
    r"(?P<charge>(?:\+|-)(?:\d+)?)?"
    r"\]"
)


@dataclass
class ParsedAtom:
    index: int
    element: str
    aromatic: bool = False
    charge: int = 0
    hcount: int | None = None


@dataclass
class ParsedBond:
    begin: int
    end: int
    order: float = 1.0
    aromatic: bool = False


@dataclass
class ParsedMol:
    atoms: list[ParsedAtom] = field(default_factory=list)
    bonds: list[ParsedBond] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)


def _parse_charge(raw: str | None) -> int:
    if not raw:
        return 0
    if raw in {"+", "-"}:
        return 1 if raw == "+" else -1
    sign = 1 if raw[0] == "+" else -1
    digits = raw[1:]
    return sign * (int(digits) if digits else 1)


def parse_organic_smiles(smiles: str) -> ParsedMol:
    """Parse a restricted organic SMILES into atoms/bonds.

    Supports: organic subset, brackets, ``-=#:``, branches ``()``, ring digits
    ``1``–``9`` / ``%NN``, aromatic lowercase, ``.`` disconnects.
    Ignores tetrahedral ``@`` / bond stereo ``/\\`` (topology only).
    """
    s = smiles.split("|", 1)[0].strip()
    if not s:
        raise ValueError("empty SMILES")

    mol = ParsedMol()
    mol.warnings.append("native SMILES parser is an organic subset for layout lab only")

    stack: list[int | None] = []
    prev: int | None = None
    bond_order = 1.0
    pending_arom = False
    rings: dict[int, tuple[int, float, bool]] = {}
    i = 0
    n = len(s)

    def add_bond(a: int, b: int, order: float, aromatic: bool) -> None:
        if a == b:
            return
        for existing in mol.bonds:
            if {existing.begin, existing.end} == {a, b}:
                return
        mol.bonds.append(ParsedBond(begin=a, end=b, order=order, aromatic=aromatic))

    while i < n:
        ch = s[i]

        if ch == "(":
            stack.append(prev)
            i += 1
            continue
        if ch == ")":
            prev = stack.pop() if stack else prev
            i += 1
            continue
        if ch == ".":
            prev = None
            bond_order = 1.0
            pending_arom = False
            i += 1
            continue
        if ch == "-":
            bond_order, pending_arom = 1.0, False
            i += 1
            continue
        if ch == "=":
            bond_order, pending_arom = 2.0, False
            i += 1
            continue
        if ch == "#":
            bond_order, pending_arom = 3.0, False
            i += 1
            continue
        if ch == ":":
            bond_order, pending_arom = 1.5, True
            i += 1
            continue
        if ch in "/\\":
            # Bond stereo — ignore for topology.
            i += 1
            continue
        if ch == "%":
            if i + 2 >= n or not s[i + 1 : i + 3].isdigit():
                raise ValueError(f"bad ring number at {i}: {s[i:]!r}")
            rnum = int(s[i + 1 : i + 3])
            i += 3
            if prev is None:
                raise ValueError("ring digit with no previous atom")
            if rnum in rings:
                other, o_order, o_arom = rings.pop(rnum)
                order = bond_order if bond_order != 1.0 else o_order
                arom = pending_arom or o_arom or (
                    mol.atoms[prev].aromatic and mol.atoms[other].aromatic
                )
                if arom and order == 1.0:
                    order = 1.5
                add_bond(prev, other, order, arom)
                bond_order, pending_arom = 1.0, False
            else:
                rings[rnum] = (prev, bond_order, pending_arom)
                bond_order, pending_arom = 1.0, False
            continue
        if ch.isdigit():
            rnum = int(ch)
            i += 1
            if prev is None:
                raise ValueError("ring digit with no previous atom")
            if rnum in rings:
                other, o_order, o_arom = rings.pop(rnum)
                order = bond_order if bond_order != 1.0 else o_order
                arom = pending_arom or o_arom or (
                    mol.atoms[prev].aromatic and mol.atoms[other].aromatic
                )
                if arom and order == 1.0:
                    order = 1.5
                add_bond(prev, other, order, arom)
                bond_order, pending_arom = 1.0, False
            else:
                rings[rnum] = (prev, bond_order, pending_arom)
                bond_order, pending_arom = 1.0, False
            continue

        # Atom
        if ch == "[":
            m = _BRACKET.match(s, i)
            if not m:
                raise ValueError(f"bad bracket atom at {i}: {s[i:i+16]!r}")
            el_raw = m.group("el")
            aromatic = el_raw.islower()
            element = el_raw.upper() if len(el_raw) == 1 else el_raw[0].upper() + el_raw[1:]
            charge = _parse_charge(m.group("charge"))
            h_raw = m.group("h")
            hcount = None
            if m.group(0).find("H") >= 0:
                hcount = int(h_raw) if h_raw else 1
            atom = ParsedAtom(
                index=len(mol.atoms),
                element=element,
                aromatic=aromatic,
                charge=charge,
                hcount=hcount,
            )
            mol.atoms.append(atom)
            i = m.end()
        else:
            # Two-letter organic first
            two = s[i : i + 2]
            if two in _ORGANIC:
                element, aromatic = _ORGANIC[two]
                i += 2
            elif ch in _ORGANIC:
                element, aromatic = _ORGANIC[ch]
                i += 1
            else:
                raise ValueError(f"unsupported SMILES token at {i}: {s[i:i+8]!r}")
            atom = ParsedAtom(index=len(mol.atoms), element=element, aromatic=aromatic)
            mol.atoms.append(atom)

        idx = atom.index
        if prev is not None:
            order = bond_order
            arom = pending_arom or (mol.atoms[prev].aromatic and atom.aromatic)
            if arom and order == 1.0:
                order = 1.5
            add_bond(prev, idx, order, arom)
        prev = idx
        bond_order = 1.0
        pending_arom = False

    if rings:
        mol.warnings.append(f"unclosed ring digits: {sorted(rings)}")
    if not mol.atoms:
        raise ValueError("no atoms parsed")
    return mol


def kekulize_aromatic_bonds(mol: ParsedMol) -> None:
    """Simple alternating Kekulé assignment on aromatic bonds (safety net)."""
    arom_bonds = [b for b in mol.bonds if b.aromatic or b.order == 1.5]
    if not arom_bonds:
        return
    # Prefer a spanning of each aromatic component with alternating 2/1.
    adj: dict[int, list[ParsedBond]] = {a.index: [] for a in mol.atoms}
    for b in arom_bonds:
        adj[b.begin].append(b)
        adj[b.end].append(b)
    seen_atoms: set[int] = set()
    for start in range(len(mol.atoms)):
        if start in seen_atoms or not adj[start]:
            continue
        # Walk a cycle/path and alternate.
        stack = [start]
        seen_atoms.add(start)
        parity = 0
        visited_bonds: set[int] = set()
        while stack:
            u = stack.pop()
            for b in adj[u]:
                bid = id(b)
                if bid in visited_bonds:
                    continue
                visited_bonds.add(bid)
                b.order = 2.0 if parity % 2 == 0 else 1.0
                b.aromatic = False
                parity += 1
                v = b.end if b.begin == u else b.begin
                if v not in seen_atoms:
                    seen_atoms.add(v)
                    stack.append(v)
