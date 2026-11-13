//! ChemAxon CXSMILES helpers for paint — built on the Lark-generated
//! [`crate::smiles_syntax`] parser (same snapshot as xenosmarts-syntax).
//!
//! Atom labels come from [`SmilesMol::atom_labels`] (trailer `$…$` **and**
//! inline `[*"Label"]`). Multicenters still use a small `m:` field scan.
//! `MOLPROP:MF_DELTA` is a molecule property ([`cx_mf_delta`]) — never an
//! atom label and never auto-promoted to a mol caption.

use crate::scene::MoleculeIn;
use crate::smiles_syntax::{parse_smiles, smiles_rt::SmilesMol};

/// One CX ``m:dummy:a.b.c`` multicenter S-group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Multicenter {
    /// Dummy / attachment-point atom index.
    pub dummy: i32,
    /// Atoms the dummy represents (ring atoms for a ring-center stem).
    pub endpts: Vec<i32>,
}

/// Parse CXSMILES ``m:dummy:a.b.c`` groups (comma-separated).
///
/// Unknown CX fields are ignored. Continuations after a split comma
/// (``m:1:2.3,4:5.6`` → ``["m:1:2.3", "4:5.6"]``) are accepted.
pub fn cx_multicenters(smiles_or_cx: &str) -> Vec<Multicenter> {
    let Some(block) = cx_block(smiles_or_cx) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut in_m = false;
    for field in split_cx_fields(block) {
        if let Some(rest) = field.strip_prefix("m:") {
            in_m = true;
            if let Some(g) = parse_m_group(rest) {
                out.push(g);
            }
        } else if in_m {
            if let Some(g) = parse_m_group(&field) {
                out.push(g);
            } else {
                in_m = false;
            }
        }
    }
    out
}

fn cx_block(smiles_or_cx: &str) -> Option<&str> {
    let trimmed = smiles_or_cx.trim();
    let start = trimmed.find('|')?;
    let rest = &trimmed[start + 1..];
    let end = rest.find('|')?;
    Some(&rest[..end])
}

fn split_cx_fields(cx: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_labels = false;
    let mut escaped = false;
    for ch in cx.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' => {
                escaped = true;
                current.push(ch);
            }
            '$' => {
                in_labels = !in_labels;
                current.push(ch);
            }
            ',' if !in_labels => {
                if !current.is_empty() {
                    fields.push(current.trim().to_string());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        fields.push(current.trim().to_string());
    }
    fields
}

fn parse_m_group(rest: &str) -> Option<Multicenter> {
    let (dummy_raw, atoms_raw) = rest.split_once(':')?;
    let dummy: i32 = dummy_raw.trim().parse().ok()?;
    if dummy < 0 {
        return None;
    }
    let mut endpts = Vec::new();
    for part in atoms_raw.split('.') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let idx: i32 = part.parse().ok()?;
        if idx < 0 {
            return None;
        }
        if !endpts.contains(&idx) {
            endpts.push(idx);
        }
    }
    if endpts.is_empty() {
        return None;
    }
    Some(Multicenter { dummy, endpts })
}

/// Display form of a ChemAxon `$…$` / inline label (strip leading `_`).
fn display_label(raw: &str) -> Option<String> {
    let p = raw.trim();
    if p.is_empty() {
        return None;
    }
    let label = if let Some(stripped) = p.strip_prefix('_') {
        if stripped.is_empty() {
            p.to_string()
        } else {
            stripped.to_string()
        }
    } else {
        p.to_string()
    };
    Some(label)
}

/// Lark-parsed per-atom labels (`$…$` trailer + inline `[*"…"]`).
///
/// Empty when the source has no labels. Prefer this over the legacy
/// trailer-only scanner.
pub fn source_atom_labels(smiles_or_cx: &str) -> Vec<Option<String>> {
    let Ok(mol) = parse_smiles(smiles_or_cx) else {
        return Vec::new();
    };
    smiles_mol_display_labels(&mol)
}

fn smiles_mol_display_labels(mol: &SmilesMol) -> Vec<Option<String>> {
    if mol.atom_labels.iter().all(|l| l.is_none()) {
        return Vec::new();
    }
    mol.atom_labels
        .iter()
        .map(|slot| slot.as_deref().and_then(display_label))
        .collect()
}

/// Parse CXSMILES ``|$a;b;c;$|`` aliases into a per-atom label list.
///
/// Prefer [`source_atom_labels`] (Lark). Kept for callers that only have a
/// trailer fragment; multi-dollar trailers (`$_AV:`) need the full parser.
pub fn cx_atom_labels(smiles_or_cx: &str) -> Vec<Option<String>> {
    source_atom_labels(smiles_or_cx)
}

/// `MOLPROP:MF_DELTA=` constraint string from a CXSMILES source (Lark IR).
///
/// Molecule property only — do not paint as an atom label or mol name.
pub fn cx_mf_delta(smiles_or_cx: &str) -> Option<String> {
    let mol = parse_smiles(smiles_or_cx).ok()?;
    mol.cx.mf_delta().map(str::to_string)
}

/// SMILES graph only — drop `` |$…$|`` / other CX trailers.
pub fn smiles_base(smiles_or_cx: &str) -> String {
    if smiles_or_cx.is_empty() {
        return String::new();
    }
    smiles_or_cx
        .split_once(" |")
        .map(|(a, _)| a)
        .unwrap_or(smiles_or_cx)
        .trim()
        .to_string()
}

/// True when the graph is a lone star placeholder (`*` / `[*]`) carrying
/// `MOLPROP:MF_DELTA`. That `*` is for other libraries — xpict keeps an
/// **empty** graph and paints the formula text only.
pub fn is_mf_delta_star_stub(smiles_or_cx: &str) -> bool {
    if cx_mf_delta(smiles_or_cx).is_none() {
        return false;
    }
    matches!(smiles_base(smiles_or_cx).as_str(), "*" | "[*]")
}

/// Star stub whose **every** OR option nets to only non-negative counts
/// (absolute formula-only). If **any** option has a loss after summing, the
/// whole constraint is a delta — all options paint with ±.
pub fn is_formula_only_constraint(constraint: &str) -> bool {
    let parts = constraint_parts(constraint);
    if parts.is_empty() {
        return false;
    }
    parts.iter().all(|p| option_is_nonneg(p))
}

fn option_is_nonneg(raw: &str) -> bool {
    let (body, _) = split_charge_suffix(raw.trim());
    match parse_delta_counts(body) {
        Ok(counts) => counts.values().all(|&n| n >= 0),
        // Unparsed body with an explicit `-` is treated as a delta.
        Err(_) => !body.contains('-'),
    }
}

/// Paint text for a star + `MF_DELTA` stub: absolute Hill only when *all*
/// OR options are nonneg; otherwise every option uses signed delta display.
pub fn format_star_mf_display(constraint: &str) -> String {
    if is_formula_only_constraint(constraint) {
        format_mf_absolute_display(constraint)
    } else {
        format_mf_delta_display(constraint)
    }
}

/// Absolute Hill spelling (chem markup). Examples: `O`, `C_{6}H_{6}O`, `O^+`.
pub fn format_mf_absolute_display(constraint: &str) -> String {
    let parts = constraint_parts(constraint);
    if parts.is_empty() {
        return String::new();
    }
    parts
        .iter()
        .map(|p| format_mf_absolute_option(p))
        .collect::<Vec<_>>()
        .join(" or ")
}

/// Display spelling for a formula-delta constraint (no Δ glyph).
///
/// One `+` / `-` per gain / loss group; counts are subscripts; charges are
/// superscripts (`^+`, `^{++}`). Examples: `+O`, `+O-H`, `+C_{6}H_{6}O`,
/// `+O or -H_{2}O`, `+O^+`.
pub fn format_mf_delta_display(constraint: &str) -> String {
    let parts = constraint_parts(constraint);
    if parts.is_empty() {
        return String::new();
    }
    parts
        .iter()
        .map(|p| format_mf_delta_option(p))
        .collect::<Vec<_>>()
        .join(" or ")
}

fn constraint_parts(constraint: &str) -> Vec<&str> {
    constraint
        .split('/')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect()
}

/// Charge as chem superscript: `^+`, `^-`, `^{++}`, `^{3+}`, …
fn format_charge_super(charge: i32) -> String {
    if charge == 0 {
        return String::new();
    }
    let plus = charge > 0;
    let mag = charge.unsigned_abs();
    let body = if mag <= 3 {
        let ch = if plus { '+' } else { '-' };
        std::iter::repeat(ch).take(mag as usize).collect::<String>()
    } else if plus {
        format!("{mag}+")
    } else {
        format!("{mag}-")
    };
    format!("^{{{}}}", body)
}

fn append_charge(mut s: String, charge: i32) -> String {
    s.push_str(&format_charge_super(charge));
    s
}

/// One absolute Hill option (`O` → `O`, `C6H6O` → `C_{6}H_{6}O`).
fn format_mf_absolute_option(raw: &str) -> String {
    let (body, charge) = split_charge_suffix(raw.trim());
    let Ok(counts) = parse_delta_counts(body) else {
        return append_charge(body.to_string(), charge);
    };
    append_charge(hill_absolute_display(&counts), charge)
}

/// One signed option: one `+` gain group and/or one `-` loss group.
fn format_mf_delta_option(raw: &str) -> String {
    let (body, charge) = split_charge_suffix(raw.trim());
    let Ok(counts) = parse_delta_counts(body) else {
        // Fall back to raw body with a leading + when it looks unsigned.
        let s = if body.starts_with('+') || body.starts_with('-') {
            body.to_string()
        } else {
            format!("+{body}")
        };
        return append_charge(s, charge);
    };
    append_charge(hill_delta_display(&counts), charge)
}

fn split_charge_suffix(s: &str) -> (&str, i32) {
    let Some(open) = s.rfind('{') else {
        return (s, 0);
    };
    if !s.ends_with('}') {
        return (s, 0);
    }
    let inner = &s[open + 1..s.len() - 1];
    let body = &s[..open];
    let charge = parse_charge_token(inner).unwrap_or(0);
    (body, charge)
}

fn parse_charge_token(inner: &str) -> Option<i32> {
    let t = inner.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(rest) = t.strip_prefix('+') {
        return Some(if rest.is_empty() {
            1
        } else {
            rest.parse().ok()?
        });
    }
    if let Some(rest) = t.strip_prefix('-') {
        return Some(if rest.is_empty() {
            -1
        } else {
            -rest.parse().ok()?
        });
    }
    if let Some(rest) = t.strip_suffix('+') {
        return Some(if rest.is_empty() {
            1
        } else {
            rest.parse().ok()?
        });
    }
    if let Some(rest) = t.strip_suffix('-') {
        return Some(if rest.is_empty() {
            -1
        } else {
            -rest.parse().ok()?
        });
    }
    None
}

fn parse_delta_counts(body: &str) -> Result<std::collections::BTreeMap<String, i32>, String> {
    let bytes = body.as_bytes();
    if bytes.is_empty() {
        return Ok(std::collections::BTreeMap::new());
    }
    let mut i = 0;
    let mut out = std::collections::BTreeMap::new();
    while i < bytes.len() {
        let mut sign: i32 = 1;
        if bytes[i] == b'-' {
            sign = -1;
            i += 1;
            if i >= bytes.len() {
                return Err("trailing sign".into());
            }
        } else if bytes[i] == b'+' {
            i += 1;
            if i >= bytes.len() {
                return Err("trailing sign".into());
            }
        }
        let c = bytes[i] as char;
        if !c.is_ascii_uppercase() {
            return Err(format!("bad delta at {c:?}"));
        }
        i += 1;
        let mut el = String::from(c);
        if i < bytes.len() {
            let c2 = bytes[i] as char;
            if c2.is_ascii_lowercase() {
                el.push(c2);
                i += 1;
            }
        }
        let mut n: i32 = 0;
        let start = i;
        while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
            n = n * 10 + (bytes[i] - b'0') as i32;
            i += 1;
        }
        if i == start {
            n = 1;
        }
        *out.entry(el).or_default() += sign * n;
    }
    out.retain(|_, v| *v != 0);
    Ok(out)
}

/// Absolute Hill (C, H, then A–Z); count `1` omitted; `n>1` → `El_{n}`.
///
/// Special case: O plus exactly one other element → that element first, O last
/// (`O3P` → `PO_{3}`, `O4S` → `SO_{4}`, `H2O` → `H_{2}O`).
fn hill_absolute_display(f: &std::collections::BTreeMap<String, i32>) -> String {
    let pos: Vec<(&str, i32)> = f
        .iter()
        .filter(|(_, &n)| n > 0)
        .map(|(el, &n)| (el.as_str(), n))
        .collect();
    let fmt = |el: &str, n: i32| -> String {
        if n == 1 {
            el.to_string()
        } else {
            format!("{el}_{{{}}}", n)
        }
    };
    // O + one other → other then O (oxide / oxoanion spelling).
    if pos.len() == 2 {
        if let Some(&(other, on)) = pos.iter().find(|(el, _)| *el != "O") {
            if let Some(&(_, o_n)) = pos.iter().find(|(el, _)| *el == "O") {
                return format!("{}{}", fmt(other, on), fmt("O", o_n));
            }
        }
    }
    let mut parts = Vec::new();
    let mut push = |el: &str| {
        let Some(&n) = f.get(el) else {
            return;
        };
        if n <= 0 {
            return;
        }
        parts.push(fmt(el, n));
    };
    let rest: Vec<_> = f
        .keys()
        .filter(|k| *k != "C" && *k != "H")
        .cloned()
        .collect();
    push("C");
    push("H");
    for el in &rest {
        push(el);
    }
    parts.concat()
}

/// Signed Hill: one `+` gain group then one `-` loss group (Hill within each).
///
/// `O` → `+O`, `O-H` → `+O-H`, `C6H6O` → `+C_{6}H_{6}O`, `-H2-O` → `-H_{2}O`.
fn hill_delta_display(f: &std::collections::BTreeMap<String, i32>) -> String {
    let mut pos = std::collections::BTreeMap::new();
    let mut neg = std::collections::BTreeMap::new();
    for (el, &n) in f {
        if n > 0 {
            pos.insert(el.clone(), n);
        } else if n < 0 {
            neg.insert(el.clone(), -n);
        }
    }
    let mut s = String::new();
    if !pos.is_empty() {
        s.push('+');
        s.push_str(&hill_absolute_display(&pos));
    }
    if !neg.is_empty() {
        s.push('-');
        s.push_str(&hill_absolute_display(&neg));
    }
    s
}

/// Apply Lark-parsed CX / inline aliases onto atoms by index.
///
/// Spoke dummies (CX ``m:``) stay unlabeled — they are paint handles at a
/// ring centroid, not ``*`` glyphs.
pub fn apply_cx_by_index(mut molecule: MoleculeIn, source: &str) -> MoleculeIn {
    let aliases = source_atom_labels(source);
    let dummies: Vec<i32> = cx_multicenters(source)
        .into_iter()
        .map(|g| g.dummy)
        .collect();
    if !aliases.is_empty() {
        for a in &mut molecule.atoms {
            if dummies.contains(&a.index) {
                a.label = Some(String::new());
                continue;
            }
            let idx = a.index as usize;
            if let Some(Some(label)) = aliases.get(idx) {
                let label = label.trim();
                if !label.is_empty() {
                    a.label = Some(label.to_string());
                }
            }
        }
    } else {
        for a in &mut molecule.atoms {
            if dummies.contains(&a.index) {
                a.label = Some(String::new());
            }
        }
    }
    molecule
}

/// CX source string for a mol node (prefer ``cxsmiles``, else ``smiles``).
pub fn cx_source<'a>(cxsmiles: Option<&'a str>, smiles: Option<&'a str>) -> Option<&'a str> {
    for s in [cxsmiles, smiles] {
        if let Some(t) = s.filter(|x| !x.trim().is_empty()) {
            return Some(t);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{AtomIn, BondIn};

    #[test]
    fn parses_cx_aliases() {
        let labels = cx_atom_labels("*C* |$;;R2;$|");
        // Lark `$…$` keeps a trailing empty slot from the final `;`.
        assert!(labels.len() >= 3);
        assert!(labels[0].is_none());
        assert!(labels[1].is_none());
        assert_eq!(labels[2].as_deref(), Some("R2"));
    }

    #[test]
    fn smiles_base_strips_trailer() {
        assert_eq!(smiles_base("*C* |$;;R2;$|"), "*C*");
    }

    #[test]
    fn bracket_inline_label_via_lark() {
        let labels = source_atom_labels(r#"CCO[*"GlcA"]"#);
        assert_eq!(labels.len(), 4);
        assert_eq!(labels[3].as_deref(), Some("GlcA"));
    }

    #[test]
    fn traced_dollar_labels_via_lark() {
        let labels =
            source_atom_labels("C(C)O[*] |$;;;GlcA$,$_AV:1;0;2;3$,atomProp:0.nextTag.4|");
        assert_eq!(labels.get(3).and_then(|l| l.as_deref()), Some("GlcA"));
    }

    #[test]
    fn mf_delta_via_lark_is_property_not_label() {
        assert_eq!(
            cx_mf_delta("* |MOLPROP:MF_DELTA=O/-H2-O|").as_deref(),
            Some("O/-H2-O")
        );
        assert_eq!(
            cx_mf_delta("Oc1ccccc1 |MOLPROP:MF_DELTA=O|").as_deref(),
            Some("O")
        );
        let labels = source_atom_labels("* |MOLPROP:MF_DELTA=O/-H2-O|");
        assert!(labels.iter().all(|l| l.is_none()) || labels.is_empty());
        assert!(is_mf_delta_star_stub("* |MOLPROP:MF_DELTA=O/-H2-O|"));
        assert!(!is_mf_delta_star_stub("Oc1ccccc1 |MOLPROP:MF_DELTA=O|"));
        assert_eq!(format_mf_delta_display("O"), "+O");
        assert_eq!(format_mf_delta_display("O-H"), "+O-H");
        assert_eq!(format_mf_delta_display("C/O"), "+C or +O");
        assert_eq!(format_mf_delta_display("O/-H2-O"), "+O or -H_{2}O");
        assert_eq!(format_mf_delta_display("O3P"), "+PO_{3}");
        assert_eq!(format_mf_delta_display("O4S"), "+SO_{4}");
        assert_eq!(format_mf_delta_display("O{+1}"), "+O^{+}");
        assert_eq!(format_mf_delta_display("O{+1}/Na{+1}"), "+O^{+} or +Na^{+}");
        assert_eq!(format_mf_delta_display("CHO-C2-H5"), "+O-CH_{4}");
        assert_eq!(format_mf_absolute_display("H2O"), "H_{2}O");
        assert_eq!(format_mf_absolute_display("O3P"), "PO_{3}");
        // Star stub: all-nonneg after sum ⇒ absolute formula (no +).
        assert!(is_formula_only_constraint("O"));
        assert!(is_formula_only_constraint("C6H6O"));
        assert!(is_formula_only_constraint("O/N"));
        assert!(is_formula_only_constraint("C6H6O/O"));
        assert!(!is_formula_only_constraint("O-H"));
        assert!(!is_formula_only_constraint("O/-H2-O"));
        assert!(!is_formula_only_constraint("CHO-C2-H5/N"));
        // OR: any loss ⇒ whole constraint is delta (every option gets ±).
        assert!(!is_formula_only_constraint("C6H6O/-H2"));
        assert!(!is_formula_only_constraint("-H2/O"));
        assert_eq!(format_star_mf_display("O"), "O");
        assert_eq!(format_star_mf_display("C6H6O"), "C_{6}H_{6}O");
        assert_eq!(format_star_mf_display("O/N"), "O or N");
        assert_eq!(format_star_mf_display("C6H6O/O"), "C_{6}H_{6}O or O");
        assert_eq!(format_star_mf_display("O/-H2-O"), "+O or -H_{2}O");
        assert_eq!(format_star_mf_display("-H2/O"), "-H_{2} or +O");
        assert_eq!(format_star_mf_display("C6H6O/-H2"), "+C_{6}H_{6}O or -H_{2}");
        assert_eq!(format_star_mf_display("O{+1}/Na{+1}"), "O^{+} or Na^{+}");
    }

    #[test]
    fn apply_cx_sets_nonempty_slots() {
        let mol = MoleculeIn {
            id: None,
            atoms: vec![
                AtomIn {
                    index: 0,
                    element: Some("*".into()),
                    z: Some(0),
                    x: 0.0,
                    y: 0.0,
                    label: Some("*".into()),
                    charge: 0,
                    radical: 0,
                    mapno: None,
                },
                AtomIn {
                    index: 1,
                    element: Some("C".into()),
                    z: Some(6),
                    x: 1.0,
                    y: 0.0,
                    label: None,
                    charge: 0,
                    radical: 0,
                    mapno: None,
                },
                AtomIn {
                    index: 2,
                    element: Some("*".into()),
                    z: Some(0),
                    x: 2.0,
                    y: 0.0,
                    label: Some("*".into()),
                    charge: 0,
                    radical: 0,
                    mapno: None,
                },
            ],
            bonds: vec![
                BondIn {
                    index: 0,
                    begin: 0,
                    end: 1,
                    order: 1.0,
                    stereo: None,
                    interior: None,
                },
                BondIn {
                    index: 1,
                    begin: 1,
                    end: 2,
                    order: 1.0,
                    stereo: None,
                    interior: None,
                },
            ],
            ..Default::default()
        };
        let out = apply_cx_by_index(mol, "*C* |$;;R2;$|");
        assert_eq!(out.atoms[2].label.as_deref(), Some("R2"));
    }

    #[test]
    fn empty_sources() {
        assert!(cx_atom_labels("").is_empty());
        assert!(cx_atom_labels("CCO").is_empty());
        assert!(cx_atom_labels("|$no close").is_empty());
        // Bare `_` is ChemAxon's empty-ish alias; display keeps `_` when
        // strip would leave nothing (same as prior paint helper).
        let labels = cx_atom_labels("*C |$_;$|");
        assert!(!labels.is_empty());
        let labels2 = cx_atom_labels("*C |$_R;$|");
        assert_eq!(labels2.first().and_then(|l| l.as_deref()), Some("R"));
    }
}
