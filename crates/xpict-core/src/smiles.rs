//! SMILES / CXSMILES as written → [`MolGraph`].
//!
//! Parse is the vendored xenosmarts-syntax reader ([`crate::smiles_syntax`]).
//! Bond conventions match xenosmarts `ToChematic`: unwritten bond between two
//! aromatic atoms is aromatic; `<-` stores dative reversed; ring-close `/` `\`
//! and dative are flipped. CX labels / `m:` stay in [`crate::cxsmiles`].

use crate::elements::atomic_number;
use crate::smiles_syntax::parse_smiles as parse_ir;
use crate::smiles_syntax::smiles_rt::{BondSym, SmilesMol};
use crate::structure::{AtomRec, BondKind, BondRec, MolGraph};

pub fn parse_smiles(input: &str) -> Result<MolGraph, String> {
    let mol = parse_ir(input)?;
    if !mol.open_rings.is_empty() {
        return Err(format!("unclosed ring {}", mol.open_rings[0].0));
    }
    if mol.atoms.is_empty() {
        return Err("empty molecule".into());
    }
    to_graph(&mol)
}

fn to_graph(mol: &SmilesMol) -> Result<MolGraph, String> {
    let mut atoms = Vec::with_capacity(mol.atoms.len());
    for a in &mol.atoms {
        let sym = a.symbol.as_str();
        let aromatic = !sym.is_empty() && sym.as_bytes()[0].is_ascii_lowercase();
        let z = if sym == "*" {
            0
        } else {
            atomic_number(sym).ok_or_else(|| format!("unknown element {sym}"))? as i32
        };
        atoms.push(AtomRec {
            z,
            charge: i32::from(a.charge),
            aromatic,
            hcount: if a.bracket {
                Some(a.hcount.unwrap_or(0))
            } else {
                None
            },
            radical: 0,
            mapno: a.atom_class.map(u32::from),
        });
    }
    let mut bonds = Vec::with_capacity(mol.bonds.len());
    for b in &mol.bonds {
        let (kind, reverse) = resolve_bond(
            &atoms,
            b.a as usize,
            b.b as usize,
            b.symbol,
            b.close_symbol,
            b.ring,
        )?;
        let (a, c) = if reverse {
            (b.b as usize, b.a as usize)
        } else {
            (b.a as usize, b.b as usize)
        };
        if bonds
            .iter()
            .any(|x: &BondRec| (x.a == a && x.b == c) || (x.a == c && x.b == a))
        {
            return Err(format!("duplicate bond {a}-{c}"));
        }
        bonds.push(BondRec { a, b: c, kind });
    }
    Ok(MolGraph { atoms, bonds })
}

fn order_of(s: BondSym) -> (BondKind, bool) {
    match s {
        BondSym::Single | BondSym::Up | BondSym::Down | BondSym::Any => (BondKind::Single, false),
        BondSym::Double => (BondKind::Double, false),
        BondSym::Triple => (BondKind::Triple, false),
        BondSym::Quadruple => (BondKind::Quadruple, false),
        BondSym::Aromatic => (BondKind::Aromatic, false),
        BondSym::DativeRight => (BondKind::Dative, false),
        BondSym::DativeLeft => (BondKind::Dative, true),
    }
}

fn flip_close(s: BondSym) -> BondSym {
    match s {
        BondSym::Up => BondSym::Down,
        BondSym::Down => BondSym::Up,
        BondSym::DativeRight => BondSym::DativeLeft,
        BondSym::DativeLeft => BondSym::DativeRight,
        other => other,
    }
}

fn resolve_bond(
    atoms: &[AtomRec],
    a: usize,
    b: usize,
    open: Option<BondSym>,
    close: Option<BondSym>,
    ring: Option<u32>,
) -> Result<(BondKind, bool), String> {
    let written = if ring.is_some() {
        match (open.map(order_of), close.map(flip_close).map(order_of)) {
            (Some(x), Some(y)) if x == y => Some(x),
            (Some(_), Some(_)) => {
                return Err(format!("ring bond conflict {}", ring.unwrap()));
            }
            (x, y) => x.or(y),
        }
    } else {
        open.map(order_of)
    };
    Ok(written.unwrap_or_else(|| {
        let arom =
            atoms.get(a).is_some_and(|x| x.aromatic) && atoms.get(b).is_some_and(|x| x.aromatic);
        (
            if arom {
                BondKind::Aromatic
            } else {
                BondKind::Single
            },
            false,
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ethanol() {
        let g = parse_smiles("CCO").unwrap();
        assert_eq!(g.atoms.len(), 3);
        assert_eq!(g.atoms[2].z, 8);
        assert_eq!(g.bonds.len(), 2);
    }

    #[test]
    fn benzene_aromatic_as_written() {
        let g = parse_smiles("c1ccccc1").unwrap();
        assert!(g.atoms.iter().all(|a| a.aromatic && a.z == 6));
        assert_eq!(g.bonds.len(), 6);
        assert!(g.bonds.iter().all(|b| b.kind == BondKind::Aromatic));
    }

    #[test]
    fn bracket_radical_h() {
        let g = parse_smiles("C[CH2]").unwrap();
        assert_eq!(g.atoms[1].hcount, Some(2));
        assert_eq!(g.atoms[0].hcount, None);
    }

    #[test]
    fn cx_trailer_stripped() {
        let g = parse_smiles("*C |$R1;$|").unwrap();
        assert_eq!(g.atoms.len(), 2);
        assert_eq!(g.atoms[0].z, 0);
    }

    #[test]
    fn disconnected_and_maps() {
        let g = parse_smiles("CC*.c1ccccc1").unwrap();
        assert_eq!(g.atoms.len(), 9);
        let g = parse_smiles("[CH3:1]").unwrap();
        assert_eq!(g.atoms[0].mapno, Some(1));
        assert_eq!(g.atoms[0].hcount, Some(3));
    }

    #[test]
    fn open_ring_errors() {
        assert!(parse_smiles("C1CC").is_err());
    }

    #[test]
    fn dative_donor_is_left() {
        let g = parse_smiles("N->[Pt]").unwrap();
        assert_eq!(g.bonds[0].kind, BondKind::Dative);
        assert_eq!(g.bonds[0].a, 0);
        let g = parse_smiles("[Pt]<-N").unwrap();
        assert_eq!(g.bonds[0].a, 1);
        assert_eq!(g.bonds[0].b, 0);
    }

    #[test]
    fn ammonium_is_n_not_nihonium() {
        let g = parse_smiles("[NH4+]").unwrap();
        assert_eq!(g.atoms.len(), 1);
        assert_eq!(g.atoms[0].z, 7);
        assert_eq!(g.atoms[0].hcount, Some(4));
        assert_eq!(g.atoms[0].charge, 1);
    }
}
