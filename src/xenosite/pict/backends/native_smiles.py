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
    r"(?P<el>\*|[A-Z][a-z]?|[a-z])"
    r"(?P<stereo>@@|@)?"
    r"(?:H(?P<h>\d?))?"
    r"(?P<charge>(?:\d+[+\-]|[+\-]\d*|[+\-]+))?"
    r"\]"
)


@dataclass
class ParsedAtom:
    index: int
    element: str
    aromatic: bool = False
    charge: int = 0
    hcount: int | None = None
    radical: int = 0
    # OpenSMILES tetrahedral: "@" anticlockwise, "@@" clockwise (from first neighbor).
    tetrahedral: str | None = None
    # Neighbor atom indices in SMILES encounter order (for stereo parity).
    smiles_neighbors: list[int] = field(default_factory=list)


@dataclass
class ParsedBond:
    begin: int
    end: int
    order: float = 1.0
    aromatic: bool = False
    # OpenSMILES directional bond ``/`` or ``\\`` (begin→end as written).
    stereo: str | None = None


@dataclass
class ParsedMol:
    atoms: list[ParsedAtom] = field(default_factory=list)
    bonds: list[ParsedBond] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)


def _parse_charge(raw: str | None) -> int:
    if not raw:
        return 0
    if raw in {"+", "++", "+++"}:
        return len(raw)
    if raw in {"-", "--", "---"}:
        return -len(raw)
    if raw[0] in "+-":
        sign = 1 if raw[0] == "+" else -1
        digits = raw[1:]
        return sign * (int(digits) if digits else 1)
    # OpenSMILES ``2+`` / ``2-``
    if raw[-1] in "+-":
        sign = 1 if raw[-1] == "+" else -1
        return sign * int(raw[:-1])
    return 0


def _infer_radical(element: str, charge: int, degree: int, hcount: int | None) -> int:
    """Unpaired electrons from bracket H underfill (e.g. ``[CH3]`` → 1)."""
    if hcount is None:
        return 0
    valence = {
        "B": 3,
        "C": 4,
        "N": 3,
        "O": 2,
        "P": 3,
        "S": 2,
        "F": 1,
        "Cl": 1,
        "Br": 1,
        "I": 1,
    }.get(element)
    if valence is None:
        return 0
    # Ammonium-like N+: treat expected sites as 4.
    expected = 4 if element == "N" and charge > 0 else valence - charge
    return max(0, expected - degree - hcount)


def parse_organic_smiles(smiles: str) -> ParsedMol:
    """Parse a restricted organic SMILES into atoms/bonds.

    Supports: organic subset, brackets, ``-=#:``, branches ``()``, ring digits
    ``1``–``9`` / ``%NN``, aromatic lowercase, ``.`` disconnects, tetrahedral
    ``@`` / ``@@`` (stored for native wedge assignment). Bond stereo ``/`` /
    ``\\`` is stored on the single bond (begin→end as written) for native E/Z
    layout enforcement.
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
    pending_stereo: str | None = None
    rings: dict[int, tuple[int, float, bool]] = {}
    i = 0
    n = len(s)

    def add_bond(
        a: int, b: int, order: float, aromatic: bool, stereo: str | None = None
    ) -> None:
        if a == b:
            return
        for existing in mol.bonds:
            if {existing.begin, existing.end} == {a, b}:
                return
        mol.bonds.append(
            ParsedBond(begin=a, end=b, order=order, aromatic=aromatic, stereo=stereo)
        )

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
            pending_stereo = None
            i += 1
            continue
        if ch == "-":
            bond_order, pending_arom = 1.0, False
            pending_stereo = None
            i += 1
            continue
        if ch == "=":
            bond_order, pending_arom = 2.0, False
            pending_stereo = None
            i += 1
            continue
        if ch == "#":
            bond_order, pending_arom = 3.0, False
            pending_stereo = None
            i += 1
            continue
        if ch == ":":
            bond_order, pending_arom = 1.5, True
            pending_stereo = None
            i += 1
            continue
        if ch in "/\\":
            # OpenSMILES directional single (E/Z). Applies to the next bond formed.
            bond_order, pending_arom = 1.0, False
            pending_stereo = ch
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
                add_bond(prev, other, order, arom, pending_stereo)
                mol.atoms[prev].smiles_neighbors.append(other)
                mol.atoms[other].smiles_neighbors.append(prev)
                bond_order, pending_arom, pending_stereo = 1.0, False, None
            else:
                rings[rnum] = (prev, bond_order, pending_arom)
                bond_order, pending_arom, pending_stereo = 1.0, False, None
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
                add_bond(prev, other, order, arom, pending_stereo)
                mol.atoms[prev].smiles_neighbors.append(other)
                mol.atoms[other].smiles_neighbors.append(prev)
                bond_order, pending_arom, pending_stereo = 1.0, False, None
            else:
                rings[rnum] = (prev, bond_order, pending_arom)
                bond_order, pending_arom, pending_stereo = 1.0, False, None
            continue

        # Atom
        if ch == "[":
            m = _BRACKET.match(s, i)
            if not m:
                raise ValueError(f"bad bracket atom at {i}: {s[i:i+16]!r}")
            el_raw = m.group("el")
            aromatic = el_raw.islower() and el_raw != "*"
            if el_raw == "*":
                element = "*"
            else:
                element = el_raw.upper() if len(el_raw) == 1 else el_raw[0].upper() + el_raw[1:]
            charge = _parse_charge(m.group("charge"))
            h_raw = m.group("h")
            hcount = None
            if "H" in m.group(0):
                hcount = int(h_raw) if h_raw else 1
            stereo = m.group("stereo")
            iso_raw = m.group("iso")
            atom = ParsedAtom(
                index=len(mol.atoms),
                element=element,
                aromatic=aromatic,
                charge=charge,
                hcount=hcount,
                tetrahedral=stereo,
            )
            mol.atoms.append(atom)
            i = m.end()
            _ = iso_raw  # isotope reserved for AtomLayout later
        else:
            # Wildcard attachment point
            if ch == "*":
                atom = ParsedAtom(index=len(mol.atoms), element="*")
                mol.atoms.append(atom)
                i += 1
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
            add_bond(prev, idx, order, arom, pending_stereo)
            mol.atoms[prev].smiles_neighbors.append(idx)
            atom.smiles_neighbors.append(prev)
        prev = idx
        bond_order = 1.0
        pending_arom = False
        pending_stereo = None

    if rings:
        mol.warnings.append(f"unclosed ring digits: {sorted(rings)}")
    if not mol.atoms:
        raise ValueError("no atoms parsed")
    # Ring closures also count as SMILES neighbors (append when closed).
    # Re-walk bonds to ensure smiles_neighbors matches graph degree; keep
    # encounter order already recorded for chain/branch, then add any missing
    # ring mates at the end (OpenSMILES ring-digit order is approximate here).
    adj: dict[int, set[int]] = {a.index: set() for a in mol.atoms}
    for b in mol.bonds:
        adj[b.begin].add(b.end)
        adj[b.end].add(b.begin)
    for a in mol.atoms:
        known = set(a.smiles_neighbors)
        for other in sorted(adj[a.index] - known):
            a.smiles_neighbors.append(other)
    # Infer radical electrons from bracket H underfill (portable; no chem engine).
    for a in mol.atoms:
        deg = len(adj[a.index])
        a.radical = _infer_radical(a.element, a.charge, deg, a.hcount)
    return mol


def implicit_h_count(atom: ParsedAtom, degree: int) -> int:
    """Default implicit H from organic valence (depiction labels)."""
    if atom.hcount is not None:
        return max(0, atom.hcount)
    # Charged atoms without explicit H: do not invent hydrogens for the label.
    if atom.charge != 0:
        return 0
    if atom.radical:
        return 0
    # Rough organic valences; aromatic atoms already counted in degree.
    valence = {
        "B": 3,
        "C": 4,
        "N": 3,
        "O": 2,
        "P": 3,
        "S": 2,
        "F": 1,
        "Cl": 1,
        "Br": 1,
        "I": 1,
    }.get(atom.element)
    if valence is None:
        return 0
    v = valence - atom.charge
    if atom.element == "N" and atom.charge == 0 and degree >= 3:
        v = 3
    return max(0, v - degree)


def atom_display_label(atom: ParsedAtom, degree: int) -> str | None:
    """RDKit-ish label: suppress plain carbon; always show stars / charge / radicals.

    Stars (``*``) always label. Charged / radical carbons show ``C``. Neutral
    heteroatoms may include implicit H (``NH``, ``OH``).
    """
    if atom.element == "*":
        return "*"
    if atom.element == "C" and atom.charge == 0 and atom.radical == 0:
        return None
    h = implicit_h_count(atom, degree)
    label = atom.element
    if h == 1:
        label += "H"
    elif h > 1:
        label += f"H{h}"
    return label


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
