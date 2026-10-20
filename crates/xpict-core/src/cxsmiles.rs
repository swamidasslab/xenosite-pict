//! ChemAxon CXSMILES atom-label and multicenter (`m:`) helpers.
//!
//! Shared by paint chrome ([`crate::doc`]) and language edges. No RDKit.
//! Chematic's CX reader ignores ``m:``; we keep it here.

use crate::scene::{Attach, MoleculeIn, SpokeIn};

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

/// Parse CXSMILES ``|$a;b;c;$|`` aliases into a per-atom label list.
///
/// Empty segments → ``None``. Returns ``[]`` when no label block is present.
/// Leading underscore in ChemAxon (``_R``) is stripped for display.
pub fn cx_atom_labels(smiles_or_cx: &str) -> Vec<Option<String>> {
    if smiles_or_cx.is_empty() {
        return Vec::new();
    }
    let Some(start) = smiles_or_cx.find("|$") else {
        return Vec::new();
    };
    let rest = &smiles_or_cx[start + 2..];
    let Some(end) = rest.find("$|") else {
        return Vec::new();
    };
    let raw = &rest[..end];
    let mut parts: Vec<&str> = raw.split(';').collect();
    if parts.last().is_some_and(|p| p.is_empty()) {
        parts.pop();
    }
    let mut out = Vec::with_capacity(parts.len());
    for p in parts {
        let p = p.trim();
        if p.is_empty() {
            out.push(None);
            continue;
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
        out.push(Some(label));
    }
    out
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

/// Apply CX aliases onto atoms by index. Only non-empty slots change labels.
///
/// Spoke dummies (CX ``m:``) stay unlabeled — they are paint handles at a
/// ring centroid, not ``*`` glyphs.
pub fn apply_cx_by_index(mut molecule: MoleculeIn, source: &str) -> MoleculeIn {
    let aliases = cx_atom_labels(source);
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
    if molecule.spokes.is_empty() {
        molecule.spokes = spoke_ins_from_source(source);
    }
    molecule
}

fn spoke_ins_from_source(source: &str) -> Vec<SpokeIn> {
    cx_multicenters(source)
        .into_iter()
        .map(|g| SpokeIn {
            dummy: Some(g.dummy),
            atom: g.dummy,
            endpts: g.endpts,
            attach: Attach::Any,
        })
        .collect()
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
        assert_eq!(labels.len(), 3);
        assert!(labels[0].is_none());
        assert!(labels[1].is_none());
        assert_eq!(labels[2].as_deref(), Some("R2"));
    }

    #[test]
    fn smiles_base_strips_trailer() {
        assert_eq!(smiles_base("*C* |$;;R2;$|"), "*C*");
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
            color: None,
            atom_shade: None,
            bond_shade: None,
            shade_vmin: None,
            shade_vmax: None,
            mark_atoms: vec![],
            mark_bonds: vec![],
            scale: 1.0,
            weight: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
            ..Default::default()
        };
        let out = apply_cx_by_index(mol, "*C* |$;;R2;$|");
        assert_eq!(out.atoms[0].label.as_deref(), Some("*"));
        assert!(out.atoms[1].label.is_none());
        assert_eq!(out.atoms[2].label.as_deref(), Some("R2"));
    }

    #[test]
    fn parses_cx_multicenter_groups() {
        let g = cx_multicenters("CC*.c1ccccc1 |m:2:3.4.5.6.7.8|");
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].dummy, 2);
        assert_eq!(g[0].endpts, vec![3, 4, 5, 6, 7, 8]);
        let two = cx_multicenters("CC*.CC*.c1ccccc1 |m:2:6.7.8.9.10.11,5:6.7.8.9.10.11|");
        assert_eq!(two.len(), 2);
        assert_eq!(two[0].dummy, 2);
        assert_eq!(two[1].dummy, 5);
        assert!(cx_multicenters("CCO").is_empty());
        assert!(cx_multicenters("CC*.c1ccccc1 |$R$|").is_empty());
    }

    #[test]
    fn empty_and_underscore_edge_cases() {
        assert!(cx_atom_labels("").is_empty());
        assert!(cx_atom_labels("CCO").is_empty());
        assert!(cx_atom_labels("|$no close").is_empty());
        let labels = cx_atom_labels("*C |$_;$|");
        assert_eq!(labels[0].as_deref(), Some("_")); // strip_prefix empty → keep
        let labels2 = cx_atom_labels("*C |$_R;$|");
        assert_eq!(labels2[0].as_deref(), Some("R"));
        assert_eq!(smiles_base(""), "");
        assert_eq!(smiles_base("CCO"), "CCO");
        assert!(cx_source(None, None).is_none());
        assert_eq!(cx_source(None, Some("  ")), None);
        assert_eq!(cx_source(Some("CX"), Some("S")), Some("CX"));
    }
}
