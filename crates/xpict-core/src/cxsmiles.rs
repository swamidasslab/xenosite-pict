//! ChemAxon CXSMILES atom-label helpers.
//!
//! Shared by paint chrome ([`crate::doc`]) and language edges. No RDKit.

use crate::scene::MoleculeIn;

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
pub fn apply_cx_by_index(mut molecule: MoleculeIn, source: &str) -> MoleculeIn {
    let aliases = cx_atom_labels(source);
    if aliases.is_empty() {
        return molecule;
    }
    for a in &mut molecule.atoms {
        let idx = a.index as usize;
        if let Some(Some(label)) = aliases.get(idx) {
            let label = label.trim();
            if !label.is_empty() {
                a.label = Some(label.to_string());
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
                },
                AtomIn {
                    index: 1,
                    element: Some("C".into()),
                    z: Some(6),
                    x: 1.0,
                    y: 0.0,
                    label: None,
                    charge: 0,
                },
                AtomIn {
                    index: 2,
                    element: Some("*".into()),
                    z: Some(0),
                    x: 2.0,
                    y: 0.0,
                    label: Some("*".into()),
                    charge: 0,
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
        };
        let out = apply_cx_by_index(mol, "*C* |$;;R2;$|");
        assert_eq!(out.atoms[0].label.as_deref(), Some("*"));
        assert!(out.atoms[1].label.is_none());
        assert_eq!(out.atoms[2].label.as_deref(), Some("R2"));
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
