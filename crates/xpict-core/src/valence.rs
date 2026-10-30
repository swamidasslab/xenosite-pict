//! OpenSMILES organic-subset implicit H (and unpaired electrons).
//!
//! Aromatic bonds count 1.5 (floored). Charge-adjusted primary valence;
//! electron-poor atoms (`C+`, `N+2`, `B`) use valence-electron count like
//! RDKit. Dative donors spend none of their covalent valence.

use crate::structure::{AtomRec, BondKind, BondRec};

/// Organic subset: B C N O F P S Cl Br I.
pub fn is_organic_subset(z: i32) -> bool {
    matches!(z, 5 | 6 | 7 | 8 | 9 | 15 | 16 | 17 | 35 | 53)
}

fn normal_valences(z: i32) -> &'static [u8] {
    match z {
        1 => &[1],
        5 => &[3],
        6 => &[4],
        7 | 15 => &[3, 5],
        8 => &[2],
        9 => &[1],
        14 => &[4],
        16 => &[2, 4, 6],
        17 | 35 | 53 => &[1, 3, 5, 7],
        33 => &[3, 5],
        34 | 52 => &[2, 4, 6],
        _ => &[],
    }
}

fn valence_electrons(z: i32) -> Option<i32> {
    match z {
        5 => Some(3),
        6 => Some(4),
        7 | 15 => Some(5),
        8 | 16 => Some(6),
        9 | 17 | 35 | 53 => Some(7),
        _ => None,
    }
}

fn order_int(kind: BondKind) -> i32 {
    match kind {
        BondKind::Single | BondKind::Dative | BondKind::Aromatic => 1,
        BondKind::Double => 2,
        BondKind::Triple => 3,
        BondKind::Quadruple => 4,
    }
}

/// Stored/implicit H for paint: bracket atoms keep `hcount`; others infer.
pub fn implicit_hcount(atoms: &[AtomRec], bonds: &[BondRec], i: usize) -> u8 {
    let atom = &atoms[i];
    if atom.z == 0 {
        return 0;
    }
    if let Some(h) = atom.hcount {
        return h;
    }
    valence_inferred_hcount(atoms, bonds, i)
}

/// Organic valence H as if the atom were unbracketed.
pub fn valence_inferred_hcount(atoms: &[AtomRec], bonds: &[BondRec], i: usize) -> u8 {
    let atom = &atoms[i];
    if atom.z == 0 || !is_organic_subset(atom.z) {
        return 0;
    }
    let valences = normal_valences(atom.z);
    if valences.is_empty() {
        return 0;
    }
    let charge = atom.charge;
    let electron_poor = valence_electrons(atom.z)
        .map(|e| e - charge)
        .filter(|&e| (0..4).contains(&e) || (e == 4 && atom.z == 5));

    let mut aromatic_count: usize = 0;
    let mut non_aromatic_sum: i32 = 0;
    for b in bonds {
        let (other, donor) = if b.a == i {
            (b.b, true)
        } else if b.b == i {
            (b.a, false)
        } else {
            continue;
        };
        let _ = other;
        if b.kind == BondKind::Aromatic {
            aromatic_count += 1;
        } else if b.kind == BondKind::Dative && donor {
            // Donor shares a lone pair; covalent valence is intact (`N->[Pt]` → NH3).
        } else {
            non_aromatic_sum += order_int(b.kind);
        }
    }

    if aromatic_count > 0 {
        let aromatic_contribution = (aromatic_count as f64 * 1.5).floor() as i32;
        let effective_sum = aromatic_contribution.saturating_add(non_aromatic_sum);
        let v = electron_poor.unwrap_or(valences[0] as i32 + charge);
        if v <= 0 || effective_sum >= v {
            return 0;
        }
        return (v - effective_sum) as u8;
    }

    let bond_sum = non_aromatic_sum;
    let valences_to_check: &[u8] = if atom.aromatic {
        &valences[..1]
    } else {
        valences
    };
    if let Some(v) = electron_poor {
        return (v - bond_sum).max(0) as u8;
    }
    for &v in valences_to_check {
        let target = v as i32 + charge;
        if target < 0 {
            continue;
        }
        if target >= bond_sum {
            return (target - bond_sum) as u8;
        }
    }
    0
}

/// Bracket H deficit vs organic valence (`[CH3]` → 1, `C[CH2]` → 1).
pub fn unpaired_electrons(atoms: &[AtomRec], bonds: &[BondRec], i: usize) -> u8 {
    valence_inferred_hcount(atoms, bonds, i).saturating_sub(implicit_hcount(atoms, bonds, i))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::{AtomRec, BondKind, BondRec};

    fn atom(z: i32, charge: i32, aromatic: bool, hcount: Option<u8>) -> AtomRec {
        AtomRec {
            z,
            charge,
            aromatic,
            hcount,
            radical: 0,
            mapno: None,
        }
    }

    fn singles(center: usize, n: usize) -> Vec<BondRec> {
        (0..n)
            .map(|k| BondRec {
                a: center,
                b: center + 1 + k,
                kind: BondKind::Single,
            })
            .collect()
    }

    fn pad(center: AtomRec, neighbors: usize) -> (Vec<AtomRec>, Vec<BondRec>) {
        let mut atoms = vec![center];
        atoms.extend((0..neighbors).map(|_| atom(6, 0, false, None)));
        (atoms, singles(0, neighbors))
    }

    #[test]
    fn methane_and_ethane() {
        let atoms = vec![atom(6, 0, false, None)];
        assert_eq!(implicit_hcount(&atoms, &[], 0), 4);
        let (atoms, bonds) = pad(atom(6, 0, false, None), 1);
        assert_eq!(implicit_hcount(&atoms, &bonds, 0), 3);
    }

    #[test]
    fn electron_poor_charged_atoms() {
        let (atoms, bonds) = pad(atom(7, 2, false, None), 1);
        assert_eq!(implicit_hcount(&atoms, &bonds, 0), 2, "C[NH2+2]");
        let (atoms, bonds) = pad(atom(6, 1, false, None), 1);
        assert_eq!(implicit_hcount(&atoms, &bonds, 0), 2, "[CH2+]C");
        let atoms = vec![atom(5, -1, false, None)];
        assert_eq!(implicit_hcount(&atoms, &[], 0), 4, "[BH4-]");
        let (atoms, bonds) = pad(atom(7, 1, false, None), 1);
        assert_eq!(implicit_hcount(&atoms, &bonds, 0), 3, "C[NH3+]");
        let (atoms, bonds) = pad(atom(8, -1, false, None), 1);
        assert_eq!(implicit_hcount(&atoms, &bonds, 0), 0, "C[O-]");
    }

    #[test]
    fn dative_donor_keeps_valence() {
        let atoms = vec![atom(7, 0, false, None), atom(78, 0, false, None)];
        let bonds = vec![BondRec {
            a: 0,
            b: 1,
            kind: BondKind::Dative,
        }];
        assert_eq!(implicit_hcount(&atoms, &bonds, 0), 3);
    }

    #[test]
    fn bracket_h_and_radical_deficit() {
        let atoms = vec![atom(6, 0, false, Some(3))];
        assert_eq!(implicit_hcount(&atoms, &[], 0), 3);
        assert_eq!(unpaired_electrons(&atoms, &[], 0), 1);
        let (atoms, bonds) = pad(atom(6, 0, false, None), 1);
        assert_eq!(unpaired_electrons(&atoms, &bonds, 0), 0);
    }

    #[test]
    fn aromatic_benzene_ch() {
        let atoms: Vec<AtomRec> = (0..6).map(|_| atom(6, 0, true, None)).collect();
        let bonds: Vec<BondRec> = (0..6)
            .map(|i| BondRec {
                a: i,
                b: (i + 1) % 6,
                kind: BondKind::Aromatic,
            })
            .collect();
        assert_eq!(implicit_hcount(&atoms, &bonds, 0), 1);
    }
}
