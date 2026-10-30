//! OpenSMILES / CXSMILES graph as written — no aromaticity perception.
//!
//! Lowercase symbols are aromatic. An unwritten bond between two aromatic
//! atoms is aromatic; `:` is aromatic. CX `|…|` trailers are skipped (labels
//! / `m:` stay in [`crate::cxsmiles`]). Conventions match xenosmarts
//! `ToChematic` without depending on that crate (Academic license).

use crate::elements::atomic_number;
use crate::structure::{AtomRec, BondKind, BondRec, MolGraph};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BondSym {
    Single,
    Double,
    Triple,
    Quadruple,
    Aromatic,
    Up,
    Down,
    Any,
    DativeRight,
    DativeLeft,
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    atoms: Vec<AtomRec>,
    bonds: Vec<BondRec>,
    prev: Option<usize>,
    pending: Option<BondSym>,
    pushed: Option<BondSym>,
    /// Open rings: (id, atom, symbol).
    rings: Vec<(u32, usize, Option<BondSym>)>,
}

pub fn parse_smiles(input: &str) -> Result<MolGraph, String> {
    let text = crate::cxsmiles::smiles_base(input);
    if text.is_empty() {
        return Err("empty molecule".into());
    }
    let mut p = Parser {
        s: text.as_bytes(),
        i: 0,
        atoms: Vec::new(),
        bonds: Vec::new(),
        prev: None,
        pending: None,
        pushed: None,
        rings: Vec::new(),
    };
    p.parse_disconnected()?;
    p.skip_ws();
    if p.i != p.s.len() {
        return Err(format!(
            "unparsed input at offset {}: {:?}",
            p.i,
            String::from_utf8_lossy(&p.s[p.i..])
        ));
    }
    if !p.rings.is_empty() {
        return Err(format!("unclosed ring {}", p.rings[0].0));
    }
    if p.atoms.is_empty() {
        return Err("empty molecule".into());
    }
    Ok(MolGraph {
        atoms: p.atoms,
        bonds: p.bonds,
    })
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.i += 1;
        Some(c)
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn err(&self, msg: &str) -> String {
        format!("{msg} at offset {}", self.i)
    }

    fn parse_disconnected(&mut self) -> Result<(), String> {
        self.parse_chain()?;
        while self.peek() == Some(b'.') {
            self.i += 1;
            self.prev = None;
            self.pending = None;
            self.parse_chain()?;
        }
        Ok(())
    }

    fn parse_chain(&mut self) -> Result<(), String> {
        self.parse_branched_atom()?;
        loop {
            match self.peek() {
                None | Some(b')') | Some(b'.') => break,
                _ => {}
            }
            let bond = self.try_bond();
            if self.is_atom_start() {
                self.pending = bond;
                self.parse_branched_atom()?;
            } else if self.is_ring_start() {
                let here = self.prev.ok_or_else(|| self.err("ring with no atom"))?;
                self.apply_ring(here, bond)?;
            } else {
                if bond.is_some() {
                    return Err(self.err("expected atom or ring after bond"));
                }
                break;
            }
        }
        Ok(())
    }

    fn parse_branched_atom(&mut self) -> Result<(), String> {
        let idx = self.parse_atom()?;
        self.connect(idx)?;
        self.prev = Some(idx);
        self.pending = None;
        loop {
            let bond = self.try_bond();
            if self.is_ring_start() {
                self.apply_ring(idx, bond)?;
            } else {
                if let Some(b) = bond {
                    self.pushed = Some(b);
                }
                break;
            }
        }
        while self.peek() == Some(b'(') {
            self.i += 1;
            let saved = self.prev;
            self.pending = self.try_bond();
            self.parse_chain()?;
            if self.bump() != Some(b')') {
                return Err(self.err("unclosed branch"));
            }
            self.prev = saved;
            self.pending = None;
        }
        Ok(())
    }

    fn connect(&mut self, idx: usize) -> Result<(), String> {
        let Some(prev) = self.prev else {
            return Ok(());
        };
        let (kind, reverse) = self.resolve_written(self.pending, None, prev, idx)?;
        self.push_bond(prev, idx, kind, reverse)?;
        Ok(())
    }

    fn apply_ring(&mut self, here: usize, close: Option<BondSym>) -> Result<(), String> {
        let n = self.parse_ring_id()?;
        if let Some(k) = self.rings.iter().position(|r| r.0 == n) {
            let (_, other, open) = self.rings.remove(k);
            let (kind, reverse) = self.resolve_written(open, close, other, here)?;
            self.push_bond(other, here, kind, reverse)?;
        } else {
            self.rings.push((n, here, close));
        }
        Ok(())
    }

    fn resolve_written(
        &self,
        open: Option<BondSym>,
        close: Option<BondSym>,
        a: usize,
        b: usize,
    ) -> Result<(BondKind, bool), String> {
        let order_of = |s: BondSym| match s {
            BondSym::Single | BondSym::Up | BondSym::Down | BondSym::Any => {
                (BondKind::Single, false)
            }
            BondSym::Double => (BondKind::Double, false),
            BondSym::Triple => (BondKind::Triple, false),
            BondSym::Quadruple => (BondKind::Quadruple, false),
            BondSym::Aromatic => (BondKind::Aromatic, false),
            BondSym::DativeRight => (BondKind::Dative, false),
            BondSym::DativeLeft => (BondKind::Dative, true),
        };
        let flip_close = |s: BondSym| match s {
            BondSym::DativeRight => BondSym::DativeLeft,
            BondSym::DativeLeft => BondSym::DativeRight,
            other => other,
        };
        let written = match (open, close.map(flip_close)) {
            (Some(x), Some(y)) => {
                let (kx, rx) = order_of(x);
                let (ky, ry) = order_of(y);
                if kx != ky || rx != ry {
                    return Err(self.err("ring bond conflict"));
                }
                Some((kx, rx))
            }
            (x, y) => x.or(y).map(order_of),
        };
        Ok(written.unwrap_or_else(|| {
            let arom = self.atoms[a].aromatic && self.atoms[b].aromatic;
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

    fn push_bond(
        &mut self,
        a: usize,
        b: usize,
        kind: BondKind,
        reverse: bool,
    ) -> Result<(), String> {
        if self
            .bonds
            .iter()
            .any(|x| (x.a == a && x.b == b) || (x.a == b && x.b == a))
        {
            return Err(format!("duplicate bond {a}-{b}"));
        }
        let (a, b) = if reverse { (b, a) } else { (a, b) };
        self.bonds.push(BondRec { a, b, kind });
        Ok(())
    }

    fn try_bond(&mut self) -> Option<BondSym> {
        if let Some(b) = self.pushed.take() {
            return Some(b);
        }
        let rest = &self.s[self.i..];
        let (sym, n) = if rest.starts_with(b"->") {
            (BondSym::DativeRight, 2)
        } else if rest.starts_with(b"<-") {
            (BondSym::DativeLeft, 2)
        } else {
            match rest.first().copied()? {
                b'-' => (BondSym::Single, 1),
                b'=' => (BondSym::Double, 1),
                b'#' => (BondSym::Triple, 1),
                b'$' => (BondSym::Quadruple, 1),
                b':' => (BondSym::Aromatic, 1),
                b'/' => (BondSym::Up, 1),
                b'\\' => (BondSym::Down, 1),
                b'~' => (BondSym::Any, 1),
                _ => return None,
            }
        };
        self.i += n;
        Some(sym)
    }

    fn is_atom_start(&self) -> bool {
        match self.peek() {
            Some(b'[') | Some(b'*') => true,
            Some(c) if c.is_ascii_alphabetic() => true,
            _ => false,
        }
    }

    fn is_ring_start(&self) -> bool {
        matches!(self.peek(), Some(b'0'..=b'9') | Some(b'%'))
    }

    fn parse_ring_id(&mut self) -> Result<u32, String> {
        if self.peek() == Some(b'%') {
            self.i += 1;
            if self.peek() == Some(b'(') {
                self.i += 1;
                let start = self.i;
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.i += 1;
                }
                if self.bump() != Some(b')') {
                    return Err(self.err("bad ring %(n)"));
                }
                return std::str::from_utf8(&self.s[start..self.i - 1])
                    .ok()
                    .and_then(|t| t.parse().ok())
                    .ok_or_else(|| self.err("bad ring number"));
            }
            let start = self.i;
            for _ in 0..2 {
                if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.i += 1;
                }
            }
            return std::str::from_utf8(&self.s[start..self.i])
                .ok()
                .and_then(|t| t.parse().ok())
                .ok_or_else(|| self.err("bad ring %nn"));
        }
        let c = self.bump().ok_or_else(|| self.err("expected ring digit"))?;
        if !c.is_ascii_digit() {
            return Err(self.err("expected ring digit"));
        }
        Ok(u32::from(c - b'0'))
    }

    fn parse_atom(&mut self) -> Result<usize, String> {
        if self.peek() == Some(b'[') {
            return self.parse_bracket();
        }
        let (z, aromatic) = self.parse_organic()?;
        let idx = self.atoms.len();
        self.atoms.push(AtomRec {
            z,
            charge: 0,
            aromatic,
            hcount: None,
            radical: 0,
            mapno: None,
        });
        Ok(idx)
    }

    fn parse_organic(&mut self) -> Result<(i32, bool), String> {
        let rest = &self.s[self.i..];
        let two = if rest.len() >= 2 {
            Some(&rest[..2])
        } else {
            None
        };
        let (sym, n) = match two {
            Some(b"Cl") => ("Cl", 2),
            Some(b"Br") => ("Br", 2),
            Some(b"se") => ("se", 2),
            Some(b"as") => ("as", 2),
            _ => match rest.first().copied() {
                Some(b'*') => ("*", 1),
                Some(c) if b"BCNOPFI".contains(&c) || b"bcnosp".contains(&c) => {
                    (std::str::from_utf8(&rest[..1]).unwrap(), 1)
                }
                _ => return Err(self.err("expected atom")),
            },
        };
        self.i += n;
        let aromatic = sym.as_bytes()[0].is_ascii_lowercase();
        let z = atomic_number(sym).ok_or_else(|| format!("unknown element {sym}"))?;
        Ok((z as i32, aromatic))
    }

    fn parse_bracket(&mut self) -> Result<usize, String> {
        self.i += 1; // [
        let mut isotope: Option<u16> = None;
        if self.peek().is_some_and(|c| c.is_ascii_digit()) {
            let start = self.i;
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.i += 1;
            }
            isotope = Some(
                std::str::from_utf8(&self.s[start..self.i])
                    .ok()
                    .and_then(|t| t.parse().ok())
                    .ok_or_else(|| self.err("bad isotope"))?,
            );
        }
        let _ = isotope;
        let (z, aromatic) = if self.peek() == Some(b'*') {
            self.i += 1;
            (0, false)
        } else {
            self.parse_bracket_symbol()?
        };
        let mut hcount = Some(0u8);
        let mut charge = 0i32;
        let mut mapno = None;
        while self.peek() != Some(b']') {
            match self.peek() {
                Some(b'@') => {
                    self.i += 1;
                    if self.peek() == Some(b'@') {
                        self.i += 1;
                    } else {
                        let rest = &self.s[self.i..];
                        if rest.starts_with(b"TH")
                            || rest.starts_with(b"AL")
                            || rest.starts_with(b"SP")
                            || rest.starts_with(b"TB")
                            || rest.starts_with(b"OH")
                        {
                            self.i += 2;
                            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                                self.i += 1;
                            }
                        }
                    }
                }
                Some(b'H') => {
                    self.i += 1;
                    if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                        let start = self.i;
                        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                            self.i += 1;
                        }
                        hcount = Some(
                            std::str::from_utf8(&self.s[start..self.i])
                                .ok()
                                .and_then(|t| t.parse().ok())
                                .ok_or_else(|| self.err("bad H count"))?,
                        );
                    } else {
                        hcount = Some(1);
                    }
                }
                Some(b'+') | Some(b'-') => {
                    charge = self.parse_charge()?;
                }
                Some(b':') => {
                    self.i += 1;
                    let start = self.i;
                    while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                        self.i += 1;
                    }
                    mapno = Some(
                        std::str::from_utf8(&self.s[start..self.i])
                            .ok()
                            .and_then(|t| t.parse().ok())
                            .ok_or_else(|| self.err("bad atom map"))?,
                    );
                }
                Some(b'"') => {
                    self.i += 1;
                    while self.peek().is_some_and(|c| c != b'"') {
                        self.i += 1;
                    }
                    if self.bump() != Some(b'"') {
                        return Err(self.err("unclosed atom label"));
                    }
                }
                Some(b']') => break,
                None => return Err(self.err("unclosed bracket")),
                _ => return Err(self.err("bad bracket atom")),
            }
        }
        if self.bump() != Some(b']') {
            return Err(self.err("unclosed bracket"));
        }
        let idx = self.atoms.len();
        self.atoms.push(AtomRec {
            z,
            charge,
            aromatic,
            hcount,
            radical: 0,
            mapno,
        });
        Ok(idx)
    }

    fn parse_bracket_symbol(&mut self) -> Result<(i32, bool), String> {
        let rest = &self.s[self.i..];
        if rest.len() >= 2 {
            let two = std::str::from_utf8(&rest[..2]).unwrap_or("");
            let b = two.as_bytes();
            // Second letter of a two-letter element is lowercase (`Cl`, `se`).
            // `NH` is nitrogen + H, not Nihonium (`Nh`).
            if b[1].is_ascii_lowercase() {
                if let Some(z) = atomic_number(two) {
                    self.i += 2;
                    return Ok((z as i32, b[0].is_ascii_lowercase()));
                }
            }
        }
        let one = rest
            .first()
            .copied()
            .ok_or_else(|| self.err("expected symbol"))?;
        if !one.is_ascii_alphabetic() {
            return Err(self.err("expected symbol"));
        }
        let s = std::str::from_utf8(&rest[..1]).unwrap();
        let z = atomic_number(s).ok_or_else(|| format!("unknown element {s}"))?;
        self.i += 1;
        Ok((z as i32, one.is_ascii_lowercase()))
    }

    fn parse_charge(&mut self) -> Result<i32, String> {
        let rest = &self.s[self.i..];
        if rest.starts_with(b"++") {
            self.i += 2;
            return Ok(2);
        }
        if rest.starts_with(b"--") {
            self.i += 2;
            return Ok(-2);
        }
        let sign = if rest.first() == Some(&b'+') { 1 } else { -1 };
        self.i += 1;
        if self.peek().is_some_and(|c| c.is_ascii_digit()) {
            let start = self.i;
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.i += 1;
            }
            let n: i32 = std::str::from_utf8(&self.s[start..self.i])
                .ok()
                .and_then(|t| t.parse().ok())
                .ok_or_else(|| self.err("bad charge"))?;
            Ok(sign * n)
        } else {
            Ok(sign)
        }
    }
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
    fn ammonium_is_n_not_nihonium() {
        let g = parse_smiles("[NH4+]").unwrap();
        assert_eq!(g.atoms.len(), 1);
        assert_eq!(g.atoms[0].z, 7);
        assert_eq!(g.atoms[0].hcount, Some(4));
        assert_eq!(g.atoms[0].charge, 1);
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
}
