//! Shared RDKit alignment protocol (MinimalLib + native Depictor).
//!
//! Alignment is **RDKit’s** `generateDepictionMatching2DStructure` /
//! MinimalLib `generate_aligned_coords`, constrained by MCS atom matches.
//!
//! Protocol:
//! 1. MCS: element + hybridization atoms; ``BondCompare: Any`` (aromatic ↔
//!    kekulé / quinone). Ring↔chain atom matches are allowed.
//! 2. Align:
//!    - Python / Rust (native): Depictor **atom map** overload (MCS SMARTS
//!      only finds matches; bond pattern is not passed as `referencePattern`).
//!    - JS MinimalLib: isotope-tag copies for MCS, then
//!      `generate_aligned_coords` with that MCS isotope ``referenceSmarts``
//!      on the **same tagged** mols (MinimalLib has no atom-map details key).
//! 3. Treat empty / `"{}"` as failure ([`align_succeeded`])

/// MinimalLib / `findMCS_P` JSON after isotope-encoding ``Z×10+hyb`` on copies.
/// Native Rust/Python do **not** use this — they set a custom AtomTyper.
pub const MCS_DETAILS_JSON: &str =
    r#"{"AtomCompare":"Isotopes","BondCompare":"Any","Timeout":2}"#;

/// Minimum MCS atom count before we trust the pattern.
pub const MIN_MCS_ATOMS: u32 = 3;

/// MinimalLib `generate_aligned_coords` details with an MCS `referenceSmarts`
/// (isotope SMARTS matched on hybridization-tagged copies — see JS
/// `layoutWithRdkit`).
pub fn minimallib_align_details(reference_smarts: &str) -> String {
    // Hand-built JSON keeps this crate free of a serde_json dependency.
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
        let d = minimallib_align_details("[62*]~[63*]");
        assert!(d.contains("referenceSmarts"));
        assert!(d.contains("[62*]~[63*]"));
        assert!(d.contains("allowRGroups\":true"));
    }

    #[test]
    fn success_predicate() {
        assert!(!align_succeeded(""));
        assert!(!align_succeeded("{}"));
        assert!(!align_succeeded("  {}  "));
        assert!(align_succeeded(r#"{"atoms":[0,1]}"#));
    }

    #[test]
    fn mcs_details_isotopes_any_for_minimallib() {
        assert!(MCS_DETAILS_JSON.contains("Isotopes"));
        assert!(MCS_DETAILS_JSON.contains("Any"));
        assert!(!MCS_DETAILS_JSON.contains("RingMatchesRingOnly"));
    }
}
