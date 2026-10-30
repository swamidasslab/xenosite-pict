//! ChemAxon CXSMILES trailer IR: what `|feature,feature,...|` wrote.
//!
//! The SMILES parser builds [`CxSmiles`] from the `cxsmiles.lark` grammar.
//! [`CxSmiles::write`] is the inverse. Features this engine does not apply
//! to a molecule stay in the IR and are listed by [`CxSmiles::dropped`].

use std::fmt;

/// One CXSMILES feature, in write order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CxField {
    AtomLabels(Vec<Option<String>>),
    AtomValues(Vec<Option<String>>),
    AtomProps(Vec<(u32, String, String)>),
    Coords(Vec<[String; 3]>),
    /// Any other spec keyword plus its payload (internal commas restored).
    Other {
        kind: CxKind,
        keyword: String,
        payload: String,
    },
}

/// Spec feature name. [`CxKind::Unknown`] is a keyword we do not list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CxKind {
    AtomLabels,
    AtomValues,
    AtomProps,
    Coords,
    LinkNodes,
    LonePairs,
    LonePairCounts,
    CoordinateBonds,
    HydrogenBonds,
    ZeroBonds,
    Radicals,
    RelativeStereo,
    StereoAbsolute,
    StereoOr,
    StereoAnd,
    Wiggly,
    WigglyUp,
    WigglyDown,
    Cis,
    Trans,
    CisTransUnspec,
    LocalParityOdd,
    LocalParityEven,
    BicycloThb,
    BicycloTlb,
    BicycloTeb,
    FragmentGroup,
    Multicenter,
    DataSGroup,
    PolymerSGroup,
    SGroupHierarchy,
    RGroups,
    RLogic,
    LigandOrder,
    QueryRb,
    QueryS,
    QueryU,
    AttachmentPoint,
    Unknown,
}

impl CxKind {
    pub fn keyword(self) -> &'static str {
        match self {
            CxKind::AtomLabels => "$labels$",
            CxKind::AtomValues => "$_AV:",
            CxKind::AtomProps => "atomProp",
            CxKind::Coords => "coords",
            CxKind::LinkNodes => "LN",
            CxKind::LonePairs => "LP",
            CxKind::LonePairCounts => "lp",
            CxKind::CoordinateBonds => "C",
            CxKind::HydrogenBonds => "H",
            CxKind::ZeroBonds => "Z",
            CxKind::Radicals => "^",
            CxKind::RelativeStereo => "r",
            CxKind::StereoAbsolute => "a",
            CxKind::StereoOr => "o",
            CxKind::StereoAnd => "&",
            CxKind::Wiggly => "w",
            CxKind::WigglyUp => "wU",
            CxKind::WigglyDown => "wD",
            CxKind::Cis => "c",
            CxKind::Trans => "t",
            CxKind::CisTransUnspec => "ctu",
            CxKind::LocalParityOdd => "@",
            CxKind::LocalParityEven => "@@",
            CxKind::BicycloThb => "THB",
            CxKind::BicycloTlb => "TLB",
            CxKind::BicycloTeb => "TEB",
            CxKind::FragmentGroup => "f",
            CxKind::Multicenter => "m",
            CxKind::DataSGroup => "SgD",
            CxKind::PolymerSGroup => "Sg",
            CxKind::SGroupHierarchy => "SgH",
            CxKind::RGroups => "RG",
            CxKind::RLogic => "LOG",
            CxKind::LigandOrder => "LO",
            CxKind::QueryRb => "rb",
            CxKind::QueryS => "s",
            CxKind::QueryU => "u",
            CxKind::AttachmentPoint => "AP",
            CxKind::Unknown => "?",
        }
    }

    /// Features [`crate::smiles_rt::SmilesMol`] applies (labels, values, atom
    /// props). Everything else is parsed and reported as dropped.
    pub fn applied(self) -> bool {
        matches!(
            self,
            CxKind::AtomLabels | CxKind::AtomValues | CxKind::AtomProps
        )
    }
}

impl fmt::Display for CxKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.keyword())
    }
}

impl CxField {
    pub fn kind(&self) -> CxKind {
        match self {
            CxField::AtomLabels(_) => CxKind::AtomLabels,
            CxField::AtomValues(_) => CxKind::AtomValues,
            CxField::AtomProps(_) => CxKind::AtomProps,
            CxField::Coords(_) => CxKind::Coords,
            CxField::Other { kind, .. } => *kind,
        }
    }

    pub fn write(&self) -> String {
        match self {
            CxField::AtomLabels(items) => format!("${}$", join_dollar(items)),
            CxField::AtomValues(items) => format!("$_AV:{}$", join_dollar(items)),
            CxField::AtomProps(ps) => {
                let body: Vec<String> = ps.iter().map(|(i, k, v)| format!("{i}.{k}.{v}")).collect();
                format!("atomProp:{}", body.join(":"))
            }
            CxField::Coords(ts) => {
                let body: Vec<String> = ts
                    .iter()
                    .map(|t| format!("{},{},{}", t[0], t[1], t[2]))
                    .collect();
                format!("({})", body.join(";"))
            }
            CxField::Other {
                keyword, payload, ..
            } => {
                if payload.is_empty() {
                    keyword.clone()
                } else {
                    format!("{keyword}:{payload}")
                }
            }
        }
    }
}

/// Parsed CXSMILES trailer (`|...|`), possibly empty.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CxSmiles {
    pub fields: Vec<CxField>,
}

impl CxSmiles {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Feature kinds present that this engine does not apply to the molecule.
    pub fn dropped(&self) -> Vec<CxKind> {
        let mut out = Vec::new();
        for f in &self.fields {
            let k = f.kind();
            if !k.applied() && !out.contains(&k) {
                out.push(k);
            }
        }
        out
    }

    /// Human-readable dropped-feature notes (`SgD`, `atomProp.color`, …).
    pub fn dropped_notes(&self) -> Vec<String> {
        let mut notes = Vec::new();
        for f in &self.fields {
            match f {
                CxField::AtomProps(ps) => {
                    for (_, k, _) in ps {
                        if k != "tag" && k != "nextTag" {
                            notes.push(format!("atomProp.{k}"));
                        }
                    }
                }
                other if !other.kind().applied() => {
                    let s = other.kind().keyword().to_string();
                    if !notes.contains(&s) {
                        notes.push(s);
                    }
                }
                _ => {}
            }
        }
        notes
    }

    pub fn labels(&self) -> Option<&[Option<String>]> {
        self.fields.iter().find_map(|f| match f {
            CxField::AtomLabels(v) => Some(v.as_slice()),
            _ => None,
        })
    }

    pub fn values(&self) -> Option<&[Option<String>]> {
        self.fields.iter().find_map(|f| match f {
            CxField::AtomValues(v) => Some(v.as_slice()),
            _ => None,
        })
    }

    pub fn atom_props(&self) -> impl Iterator<Item = &(u32, String, String)> {
        self.fields.iter().flat_map(|f| match f {
            CxField::AtomProps(v) => v.iter(),
            _ => [].iter(),
        })
    }

    /// `|feature,feature,...|` including the bars. Empty IR writes nothing.
    pub fn write(&self) -> String {
        if self.fields.is_empty() {
            return String::new();
        }
        let body: Vec<String> = self.fields.iter().map(CxField::write).collect();
        format!("|{}|", body.join(","))
    }

    /// Classify a keyword + payload (internal commas already joined).
    pub fn classify(kw: &str, payload: String) -> CxField {
        match kw {
            "atomProp" | "atomprop" => CxField::AtomProps(parse_atom_props(&payload)),
            _ => CxField::Other {
                kind: kind_of(kw),
                keyword: kw.to_string(),
                payload,
            },
        }
    }
}

pub fn kind_of(kw: &str) -> CxKind {
    match kw {
        "LN" => CxKind::LinkNodes,
        "LP" => CxKind::LonePairs,
        "lp" => CxKind::LonePairCounts,
        "C" => CxKind::CoordinateBonds,
        "H" => CxKind::HydrogenBonds,
        "Z" => CxKind::ZeroBonds,
        k if k.starts_with('^') => CxKind::Radicals,
        "r" => CxKind::RelativeStereo,
        "a" => CxKind::StereoAbsolute,
        k if k.starts_with('o') && k.as_bytes().get(1).is_none_or(|b| b.is_ascii_digit()) => {
            CxKind::StereoOr
        }
        k if k.starts_with('&') => CxKind::StereoAnd,
        "w" => CxKind::Wiggly,
        "wU" => CxKind::WigglyUp,
        "wD" => CxKind::WigglyDown,
        "c" => CxKind::Cis,
        "t" => CxKind::Trans,
        "ctu" => CxKind::CisTransUnspec,
        "@" => CxKind::LocalParityOdd,
        "@@" => CxKind::LocalParityEven,
        "THB" => CxKind::BicycloThb,
        "TLB" => CxKind::BicycloTlb,
        "TEB" => CxKind::BicycloTeb,
        "f" => CxKind::FragmentGroup,
        "m" => CxKind::Multicenter,
        "SgD" => CxKind::DataSGroup,
        "Sg" => CxKind::PolymerSGroup,
        "SgH" => CxKind::SGroupHierarchy,
        "RG" => CxKind::RGroups,
        "LOG" => CxKind::RLogic,
        "LO" => CxKind::LigandOrder,
        "rb" => CxKind::QueryRb,
        "s" => CxKind::QueryS,
        "u" => CxKind::QueryU,
        k if k.starts_with("AP") => CxKind::AttachmentPoint,
        _ => CxKind::Unknown,
    }
}

pub fn parse_atom_props(body: &str) -> Vec<(u32, String, String)> {
    let mut out = Vec::new();
    for piece in body.split(':') {
        if piece.is_empty() {
            continue;
        }
        let Some((idx_s, rest)) = piece.split_once('.') else {
            continue;
        };
        let Some((name, value)) = rest.split_once('.') else {
            continue;
        };
        let Ok(i) = idx_s.parse() else { continue };
        out.push((i, unescape_dollar(name), unescape_dollar(value)));
    }
    out
}

pub fn split_dollar(inner: &str) -> Vec<Option<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let b = inner.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            cur.push(b[i + 1] as char);
            i += 2;
            continue;
        }
        if b[i] == b'&' && i + 1 < b.len() && b[i + 1] == b'#' {
            if let Some(end) = inner[i + 2..].find(';') {
                if let Ok(n) = inner[i + 2..i + 2 + end].parse::<u32>() {
                    if let Some(c) = char::from_u32(n) {
                        cur.push(c);
                        i += 3 + end;
                        continue;
                    }
                }
            }
        }
        if b[i] == b';' {
            out.push((!cur.is_empty()).then(|| std::mem::take(&mut cur)));
            i += 1;
            continue;
        }
        cur.push(b[i] as char);
        i += 1;
    }
    out.push((!cur.is_empty()).then_some(cur));
    out
}

fn join_dollar(items: &[Option<String>]) -> String {
    items
        .iter()
        .map(|s| s.as_deref().map(escape_dollar).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(";")
}

/// ChemAxon `&#n;` plus a backslash escape (records we already wrote).
pub fn unescape_dollar(s: &str) -> String {
    let mut out = String::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            out.push(b[i + 1] as char);
            i += 2;
            continue;
        }
        if b[i] == b'&' && i + 1 < b.len() && b[i + 1] == b'#' {
            if let Some(end) = s[i + 2..].find(';') {
                if let Ok(n) = s[i + 2..i + 2 + end].parse::<u32>() {
                    if let Some(c) = char::from_u32(n) {
                        out.push(c);
                        i += 3 + end;
                        continue;
                    }
                }
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

/// Spec: `$`, `;`, `|`, `{`, `}` inside a `$...$` block are `&#n;`.
pub fn escape_dollar(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if matches!(c, '$' | ';' | '|' | '{' | '}') {
            out.push_str(&format!("&#{};", u32::from(c)));
        } else {
            out.push(c);
        }
    }
    out
}

pub fn parse_coords(inner: &str) -> Vec<[String; 3]> {
    inner
        .split(';')
        .filter(|t| !t.is_empty() || inner.contains(','))
        .map(|t| {
            let mut p = t.split(',');
            let x = p.next().unwrap_or("").to_string();
            let y = p.next().unwrap_or("").to_string();
            let z = p.next().unwrap_or("").to_string();
            [x, y, z]
        })
        .filter(|t| !(t[0].is_empty() && t[1].is_empty() && t[2].is_empty()))
        .collect()
}

/// Builder pieces from the generated parser, joined then classified.
#[derive(Debug, Clone)]
pub enum CxPiece {
    Dollar(String),
    Coords(String),
    Kw { name: String, payload: String },
    Cont(String),
}

impl CxSmiles {
    pub fn from_pieces(pieces: Vec<CxPiece>) -> Self {
        let mut raw: Vec<(String, String)> = Vec::new();
        for p in pieces {
            match p {
                CxPiece::Dollar(inner) => raw.push(("$".into(), inner)),
                CxPiece::Coords(inner) => raw.push(("(".into(), inner)),
                CxPiece::Kw { name, payload } => raw.push((name, payload)),
                CxPiece::Cont(s) => {
                    if let Some((_, pay)) = raw.last_mut() {
                        if !pay.is_empty() {
                            pay.push(',');
                        }
                        pay.push_str(&s);
                    } else {
                        raw.push(("?".into(), s));
                    }
                }
            }
        }
        let mut fields = Vec::new();
        for (name, payload) in raw {
            if name == "$" {
                if let Some(rest) = payload.strip_prefix("_AV:") {
                    fields.push(CxField::AtomValues(split_dollar(rest)));
                } else {
                    fields.push(CxField::AtomLabels(split_dollar(&payload)));
                }
            } else if name == "(" {
                fields.push(CxField::Coords(parse_coords(&payload)));
            } else {
                fields.push(CxSmiles::classify(&name, payload));
            }
        }
        CxSmiles { fields }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dollar_round_trip_unescape() {
        assert_eq!(unescape_dollar("Protein&#59;x"), "Protein;x");
        assert_eq!(unescape_dollar(r"Protein\;x"), "Protein;x");
        assert_eq!(escape_dollar("Protein;x"), "Protein&#59;x");
        assert_eq!(
            split_dollar(r"GlcA;;;;;;Protein\;x"),
            vec![
                Some("GlcA".into()),
                None,
                None,
                None,
                None,
                None,
                Some("Protein;x".into()),
            ]
        );
        assert_eq!(
            split_dollar("GlcA;;;;;;Protein&#59;x"),
            vec![
                Some("GlcA".into()),
                None,
                None,
                None,
                None,
                None,
                Some("Protein;x".into()),
            ]
        );
    }

    #[test]
    fn assemble_sgd_commas() {
        let cx = CxSmiles::from_pieces(vec![
            CxPiece::Kw {
                name: "SgD".into(),
                payload: "0".into(),
            },
            CxPiece::Cont("1".into()),
            CxPiece::Cont("2:name:data".into()),
        ]);
        assert_eq!(cx.fields.len(), 1);
        assert_eq!(cx.dropped(), vec![CxKind::DataSGroup]);
        assert_eq!(cx.write(), "|SgD:0,1,2:name:data|");
    }
}
