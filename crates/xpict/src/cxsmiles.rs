//! ChemAxon CXSMILES atom-label helpers (parity with JS / Python).

/// Parse CXSMILES ``|$a;b;c;$|`` aliases into a per-atom label list.
/// Empty segments → ``None``. Returns ``[]`` when no label block is present.
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
