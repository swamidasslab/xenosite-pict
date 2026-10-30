//! Runtime for the generated SMILES parser (`gen_smiles.rs`).
//!
//! Parsing builds a [`SmilesMol`]: SMILES *as written*, with no perception
//! and no validation (mirrors dev/smarts_grammar/smiles.py). The SMILES
//! conventions and a molecule model are applied afterwards, by the engine's
//! `ToChematic`, so the parser never branches on chemistry -- and this crate
//! never names a chemistry type.

use winnow::stream::Stateful;

pub use super::cxsmiles::{CxField, CxKind, CxPiece, CxSmiles};
pub use super::runtime::{lit, num, tok, PR};
use winnow::error::{ContextError, ErrMode};

pub type In<'i, 's> = Stateful<&'i str, &'s mut SmilesBuilder>;

// ------------------------------------------------------------------ types

/// Element symbol as written (`C`, `c`, `Cl`, `se`, `*`); at most 2 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sym([u8; 2], u8);

impl Sym {
    pub fn new(s: &str) -> Self {
        let b = s.as_bytes();
        let mut a = [0u8; 2];
        a[..b.len()].copy_from_slice(b);
        Sym(a, b.len() as u8)
    }
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0[..self.1 as usize]).unwrap()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SmilesAtom {
    pub symbol: Sym,
    pub bracket: bool,
    pub isotope: Option<u16>,
    /// Chirality token as written (`@`, `@@`, `@SP1`, ...).
    pub chirality: Option<Box<str>>,
    /// Bracket H count (0 if none written); `None` = organic subset.
    pub hcount: Option<u8>,
    pub charge: i8,
    pub atom_class: Option<u16>,
    /// Inline `[*"text"]` label as written; merged into
    /// [`SmilesMol::atom_labels`], the field the CXSMILES `$...$` trailer fills.
    pub label: Option<Box<str>>,
}

impl SmilesAtom {
    pub fn organic(s: &str) -> Self {
        SmilesAtom {
            symbol: Sym::new(s),
            bracket: false,
            isotope: None,
            chirality: None,
            hcount: None,
            charge: 0,
            atom_class: None,
            label: None,
        }
    }
    pub fn bracket(
        isotope: Option<u16>,
        s: &str,
        label: Option<&str>,
        chirality: Option<&str>,
        h: Option<u8>,
        charge: Option<i8>,
        class: Option<u16>,
    ) -> Self {
        SmilesAtom {
            symbol: Sym::new(s),
            bracket: true,
            isotope,
            chirality: chirality.map(Into::into),
            hcount: Some(h.unwrap_or(0)),
            charge: charge.unwrap_or(0),
            atom_class: class,
            // empty and absent are one state on a molecule
            label: label
                .map(|l| &l[1..l.len() - 1])
                .filter(|l| !l.is_empty())
                .map(Into::into),
        }
    }
}

/// Bond symbol as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondSym {
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

impl BondSym {
    pub fn parse(s: &str) -> Self {
        match s {
            "-" => BondSym::Single,
            "=" => BondSym::Double,
            "#" => BondSym::Triple,
            "$" => BondSym::Quadruple,
            ":" => BondSym::Aromatic,
            "/" => BondSym::Up,
            "\\" => BondSym::Down,
            "~" => BondSym::Any,
            "->" => BondSym::DativeRight,
            "<-" => BondSym::DativeLeft,
            _ => unreachable!("BOND terminal"),
        }
    }
    pub fn text(self) -> &'static str {
        match self {
            BondSym::Single => "-",
            BondSym::Double => "=",
            BondSym::Triple => "#",
            BondSym::Quadruple => "$",
            BondSym::Aromatic => ":",
            BondSym::Up => "/",
            BondSym::Down => "\\",
            BondSym::Any => "~",
            BondSym::DativeRight => "->",
            BondSym::DativeLeft => "<-",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SmilesBond {
    pub a: u32,
    pub b: u32,
    /// As written (`None` = no symbol). Ring bonds: at the opening digit.
    pub symbol: Option<BondSym>,
    pub ring: Option<u32>,
    /// Ring bonds: symbol written at the closing digit (read close→open).
    pub close_symbol: Option<BondSym>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StereoOrder {
    pub atom: u32,
    /// `neighbors[0]` is the atom this one was written after.
    pub has_from: bool,
    /// Written order; `None` = ring partner that never closed.
    pub neighbors: Vec<Option<u32>>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SmilesMol {
    pub atoms: Vec<SmilesAtom>,
    pub bonds: Vec<SmilesBond>,
    pub component: Vec<u32>,
    pub stereo: Vec<StereoOrder>,
    /// (ring number, atom, symbol) opened and never closed — recorded, not rejected.
    pub open_rings: Vec<(u32, u32, Option<BondSym>)>,
    /// CXSMILES `$...$` labels in written atom order (`None` = empty).
    pub atom_labels: Vec<Option<String>>,
    /// Other CXSMILES fields, as written (`$_AV:` / `atomProp` included).
    /// Derived from [`SmilesMol::cx`] so JSON parity stays a list of strings.
    pub cx_fields: Vec<String>,
    /// Parsed CXSMILES trailer. Empty when the string had no `|...|`.
    pub cx: CxSmiles,
}

/// What `atom` / `ring` hand to `bond_atom`.
#[derive(Debug, Clone)]
pub enum Target {
    Atom(SmilesAtom),
    Ring(u32),
}

// ---------------------------------------------------------------- builder

const NOT_CHIRAL: u32 = u32::MAX;

#[derive(Debug, Default)]
pub struct SmilesBuilder {
    pub mol: SmilesMol,
    pub prev: Option<u32>,
    comp: u32,
    started: bool,
    /// Open ring digits in opening order: (number, atom, symbol, slot).
    rings: Vec<(u32, u32, Option<BondSym>, u32)>,
    /// slot -> (index into mol.stereo, position) for a chiral opener.
    pending: Vec<Option<(u32, u32)>>,
    /// atom -> index into mol.stereo (NOT_CHIRAL if none).
    stereo_of: Vec<u32>,
}

impl SmilesBuilder {
    pub fn begin_mol(&mut self) {
        if self.started {
            self.comp += 1;
        }
        self.started = true;
        self.prev = None;
    }

    fn add_atom(&mut self, a: SmilesAtom, symbol: Option<BondSym>) {
        let idx = self.mol.atoms.len() as u32;
        let chiral = a.chirality.is_some();
        self.mol.atoms.push(a);
        self.mol.component.push(self.comp);
        if chiral {
            self.stereo_of.push(self.mol.stereo.len() as u32);
            self.mol.stereo.push(StereoOrder {
                atom: idx,
                has_from: self.prev.is_some(),
                neighbors: self.prev.map(Some).into_iter().collect(),
            });
        } else {
            self.stereo_of.push(NOT_CHIRAL);
        }
        if let Some(p) = self.prev {
            self.mol.bonds.push(SmilesBond {
                a: p,
                b: idx,
                symbol,
                ring: None,
                close_symbol: None,
            });
            let s = self.stereo_of[p as usize];
            if s != NOT_CHIRAL {
                self.mol.stereo[s as usize].neighbors.push(Some(idx));
            }
        }
        self.prev = Some(idx);
    }

    pub fn first_atom(&mut self, t: Target) {
        match t {
            Target::Atom(a) => self.add_atom(a, None),
            Target::Ring(_) => unreachable!("first_atom is an atom"),
        }
    }

    pub fn bond_atom(&mut self, bond: Option<&str>, t: Target) {
        let symbol = bond.map(BondSym::parse);
        match t {
            Target::Atom(a) => self.add_atom(a, symbol),
            Target::Ring(n) => self.ring(n, symbol),
        }
    }

    fn ring(&mut self, n: u32, symbol: Option<BondSym>) {
        let here = self.prev.expect("ring digit after an atom");
        let s_here = self.stereo_of[here as usize];
        if let Some(k) = self.rings.iter().position(|r| r.0 == n) {
            let (_, other, open_symbol, slot) = self.rings.remove(k);
            self.mol.bonds.push(SmilesBond {
                a: other,
                b: here,
                symbol: open_symbol,
                ring: Some(n),
                close_symbol: symbol,
            });
            if s_here != NOT_CHIRAL {
                self.mol.stereo[s_here as usize].neighbors.push(Some(other));
            }
            if let Some((s, pos)) = self.pending[slot as usize] {
                self.mol.stereo[s as usize].neighbors[pos as usize] = Some(here);
            }
        } else {
            let slot = self.pending.len() as u32;
            if s_here != NOT_CHIRAL {
                let nb = &mut self.mol.stereo[s_here as usize].neighbors;
                self.pending.push(Some((s_here, nb.len() as u32)));
                nb.push(None);
            } else {
                self.pending.push(None);
            }
            self.rings.push((n, here, symbol, slot));
        }
    }

    /// CXSMILES `|feature,...|` pieces from the generated trailer parser.
    pub fn set_cx(&mut self, pieces: Vec<CxPiece>) {
        let cx = CxSmiles::from_pieces(pieces);
        if let Some(labels) = cx.labels() {
            self.mol.atom_labels = labels.to_vec();
        }
        self.mol.cx_fields = cx
            .fields
            .iter()
            .filter_map(|f| match f {
                CxField::AtomLabels(_) => None,
                other => Some(other.write()),
            })
            .collect();
        self.mol.cx = cx;
    }

    pub fn finish(mut self) -> Result<SmilesMol, String> {
        self.mol.open_rings = self.rings.iter().map(|r| (r.0, r.1, r.2)).collect();
        self.mol.merge_inline_labels()?;
        Ok(self.mol)
    }
}

// ------------------------------------------------------- parse helpers

/// The label of a bracket atom, written after the symbol or last. Writing
/// one in both places is refused rather than resolved.
pub fn label<'i>(after_symbol: Option<&'i str>, last: Option<&'i str>) -> PR<Option<&'i str>> {
    match (after_symbol, last) {
        (Some(_), Some(_)) => Err(ErrMode::Backtrack(ContextError::new())),
        (a, b) => Ok(a.or(b)),
    }
}

pub fn ring_number(text: &str) -> u32 {
    num(text
        .trim_start_matches('%')
        .trim_start_matches('(')
        .trim_end_matches(')'))
}

pub fn hcount(text: &str) -> u8 {
    if text.len() > 1 {
        num(&text[1..]) as u8
    } else {
        1
    }
}

pub fn charge(text: &str) -> i8 {
    match text {
        "++" => 2,
        "--" => -2,
        _ => {
            let sign = if text.starts_with('+') { 1 } else { -1 };
            sign * if text.len() > 1 {
                num(&text[1..]) as i8
            } else {
                1
            }
        }
    }
}

// ------------------------------------------------------------------ JSON

fn opt<T: ToString>(v: &Option<T>) -> String {
    v.as_ref().map_or("null".into(), |x| x.to_string())
}

fn qstr(s: &str) -> String {
    format!("{s:?}")
}

impl SmilesMol {
    /// An inline `[*"text"]` fills the same per-atom slot the CXSMILES
    /// `$...$` trailer fills, so one atom cannot be labelled twice. Run once
    /// the trailer has been read.
    fn merge_inline_labels(&mut self) -> Result<(), String> {
        if self.atoms.iter().all(|a| a.label.is_none()) {
            return Ok(());
        }
        self.atom_labels.resize(self.atoms.len(), None);
        for (i, a) in self.atoms.iter().enumerate() {
            let Some(l) = &a.label else { continue };
            if self.atom_labels[i].is_some() {
                return Err(format!(
                    "atom {i} is labelled both inline and in the CXSMILES trailer"
                ));
            }
            self.atom_labels[i] = Some(l.to_string());
        }
        Ok(())
    }

    pub fn to_json(&self) -> String {
        let atoms: Vec<String> = self
            .atoms
            .iter()
            .map(|a| {
                format!(
                    "[{},{},{},{},{},{},{}]",
                    qstr(a.symbol.as_str()),
                    a.bracket,
                    opt(&a.isotope),
                    a.chirality.as_deref().map_or("null".into(), qstr),
                    opt(&a.hcount),
                    a.charge,
                    opt(&a.atom_class)
                )
            })
            .collect();
        let sym = |s: &Option<BondSym>| s.map_or("null".into(), |s| qstr(s.text()));
        let bonds: Vec<String> = self
            .bonds
            .iter()
            .map(|b| {
                format!(
                    "[{},{},{},{},{}]",
                    b.a,
                    b.b,
                    sym(&b.symbol),
                    opt(&b.ring),
                    sym(&b.close_symbol)
                )
            })
            .collect();
        let stereo: Vec<String> = self
            .stereo
            .iter()
            .map(|s| {
                let nb: Vec<String> = s.neighbors.iter().map(opt).collect();
                format!("[{},{},[{}]]", s.atom, s.has_from, nb.join(","))
            })
            .collect();
        let open: Vec<String> = self
            .open_rings
            .iter()
            .map(|r| format!("[{},{},{}]", r.0, r.1, sym(&r.2)))
            .collect();
        let labels: Vec<String> = self
            .atom_labels
            .iter()
            .map(|l| l.as_deref().map_or("null".into(), qstr))
            .collect();
        let cx: Vec<String> = self.cx_fields.iter().map(|f| qstr(f)).collect();
        let comp: Vec<String> = self.component.iter().map(|c| c.to_string()).collect();
        format!(
            r#"{{"atoms":[{}],"bonds":[{}],"component":[{}],"stereo":[{}],"open_rings":[{}],"atom_labels":[{}],"cx_fields":[{}]}}"#,
            atoms.join(","),
            bonds.join(","),
            comp.join(","),
            stereo.join(","),
            open.join(","),
            labels.join(","),
            cx.join(",")
        )
    }
}
