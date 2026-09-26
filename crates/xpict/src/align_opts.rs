//! Shared alignment protocol (MinimalLib + native Depictor).
//!
//! Alignment is **RDKit’s** `generateDepictionMatching2DStructure` /
//! MinimalLib `generate_aligned_coords`, constrained by MCS atom matches.
//!
//! Protocol:
//! 1. **MCS** (chematic in `xpict-core`, not host RDKit FMCS): element +
//!    hybridization via isotope tags ``Z×10+hyb``; any-bond. See
//!    [`xpict_core::mcs_atom_map`]. Chematic is **not** used for 2D coords.
//! 2. Align:
//!    - Python / Rust (native): Depictor **atom map** overload.
//!    - JS MinimalLib: unique-isotope tags from the atom map, then
//!      `generate_aligned_coords` with that isotope ``referenceSmarts``
//!      (MinimalLib has no atom-map details key).
//! 3. Treat empty / `"{}"` as failure ([`align_succeeded`])

/// MinimalLib details for isotope-tagged copies (legacy helper; MCS itself
/// is chematic). Kept for MinimalLib `generate_aligned_coords` SMARTS bridge.
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
