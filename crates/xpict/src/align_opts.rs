//! Shared RDKit alignment protocol (MinimalLib + native Depictor).
//!
//! Alignment is **RDKit’s** `generateDepictionMatching2DStructure` /
//! MinimalLib `generate_aligned_coords`, constrained by an FMCS SMARTS with
//! ``BondCompare: Any`` so aromatic rings match kekulé / quinone forms.
//!
//! After MCS, language edges must reject **saturated ↔ unsaturated** matches
//! (e.g. cyclohexane vs benzoquinone) — see Python
//! ``_mcs_saturation_compatible`` / JS ``mcsSaturationCompatible``.
//!
//! Language bindings should not reinvent Kabsch — call RDKit with these
//! option shapes:
//!
//! 1. MCS: [`MCS_DETAILS_JSON`] + saturation filter
//! 2. Align: [`minimallib_align_details`] → `generate_aligned_coords`
//! 3. Treat empty / `"{}"` as failure ([`align_succeeded`])

/// MinimalLib / `findMCS_P` JSON: element atoms, any-bond (aromatic ↔ order).
/// Pair with the sat↔unsat filter in Python/JS — not sufficient alone.
pub const MCS_DETAILS_JSON: &str =
    r#"{"AtomCompare":"Elements","BondCompare":"Any","Timeout":2}"#;

/// Minimum MCS atom count before we trust the pattern.
pub const MIN_MCS_ATOMS: u32 = 3;

/// MinimalLib `generate_aligned_coords` details with an MCS `referenceSmarts`.
pub fn minimallib_align_details(reference_smarts: &str) -> String {
    // Hand-built JSON keeps this crate free of a serde_json dependency for
    // a two-field object. Escape is unnecessary: SMARTS use `# [ ] : = -` etc.
    format!(
        r#"{{"useCoordGen":false,"referenceSmarts":"{}","allowRGroups":true,"acceptFailure":false}}"#,
        escape_json_string(reference_smarts)
    )
}

/// MinimalLib returns `""` on hard failure and `"{}"` when
/// ``acceptFailure`` produced an unconstrained layout.
pub fn align_succeeded(result: &str) -> bool {
    let t = result.trim();
    !t.is_empty() && t != "{}"
}

fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn details_embeds_smarts() {
        let d = minimallib_align_details("[#6]1:[#6]:[#6]:1");
        assert!(d.contains("referenceSmarts"));
        assert!(d.contains("[#6]1:[#6]:[#6]:1"));
        assert!(d.contains("allowRGroups\":true"));
    }

    #[test]
    fn success_predicate() {
        assert!(!align_succeeded(""));
        assert!(!align_succeeded("{}"));
        assert!(!align_succeeded("  {}  "));
        assert!(align_succeeded(r#"{"atoms":[0,1]}"#));
    }
}
