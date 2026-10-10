//! Alignment debug overlays — what matched what.
//!
//! When [`crate::scene::MoleculeIn::debug_align`] is on and
//! [`crate::scene::AlignDebug`] is stamped, depict paints:
//! - **Atom level:** each matched atom gets a small disk + the **template atom
//!   index** as text. Same number on template and query ⇒ same correspondence.
//!   Unmatched query atoms get a muted open circle.
//! - **Mol level:** a corner badge with role + peer id(s).

use crate::scene::{AlignDebug, AlignDebugRole, AtomIn, Primitive, TextAnchor};

pub const DEBUG_ALIGN_OPACITY: f64 = 0.75;
pub const DEBUG_ALIGN_UNMATCHED: &str = "#9e9e9e";
pub const DEBUG_ALIGN_BADGE: &str = "#1565c0";

/// Stable palette — index by ``tag.rem_euclid(N)``.
pub const DEBUG_ALIGN_PALETTE: &[&str] = &[
    "#e53935", "#8e24aa", "#3949ab", "#039be5", "#00897b", "#43a047", "#f9a825",
    "#fb8c00", "#6d4c41", "#546e7a", "#c0ca33", "#d81b60",
];

pub fn tag_color(tag: i32) -> &'static str {
    let i = tag.rem_euclid(DEBUG_ALIGN_PALETTE.len() as i32) as usize;
    DEBUG_ALIGN_PALETTE[i]
}

/// Build template-side debug: every mapped ref atom tagged with its own index.
pub fn align_debug_template(peer_ids: &[String], maps: &[Vec<(i32, i32)>]) -> AlignDebug {
    let mut tags: Vec<(i32, i32)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for map in maps {
        for &(_q, r) in map {
            if seen.insert(r) {
                tags.push((r, r));
            }
        }
    }
    tags.sort_by_key(|(a, _)| *a);
    AlignDebug {
        role: AlignDebugRole::Template,
        peer_id: if peer_ids.is_empty() {
            None
        } else {
            Some(peer_ids.join(","))
        },
        tags,
    }
}

/// Build query-side debug: each query atom tagged with the **template** atom index.
pub fn align_debug_query(peer_id: &str, query_to_ref: &[(i32, i32)]) -> AlignDebug {
    let mut tags: Vec<(i32, i32)> = query_to_ref
        .iter()
        .map(|&(q, r)| (q, r))
        .collect();
    tags.sort_by_key(|(a, _)| *a);
    AlignDebug {
        role: AlignDebugRole::Query,
        peer_id: Some(peer_id.to_string()),
        tags,
    }
}

/// Paint align-debug primitives in molecule (canvas) space after ``dx, dy`` shift.
pub fn paint_align_debug(
    atoms: &[AtomIn],
    dbg: &AlignDebug,
    dx: f64,
    dy: f64,
    font_px: f64,
) -> Vec<Primitive> {
    let mut out = Vec::new();
    let by: std::collections::HashMap<i32, &AtomIn> =
        atoms.iter().map(|a| (a.index, a)).collect();
    let matched: std::collections::HashSet<i32> = dbg.tags.iter().map(|(a, _)| *a).collect();
    let r = (font_px * 0.45).max(3.0);

    if dbg.role == AlignDebugRole::Query {
        for a in atoms {
            if matched.contains(&a.index) {
                continue;
            }
            out.push(Primitive::Circle {
                cx: a.x + dx,
                cy: a.y + dy,
                r: r * 0.7,
                fill: Some("none".into()),
                stroke: Some(DEBUG_ALIGN_UNMATCHED.into()),
                stroke_width: 1.0,
                opacity: 0.55,
                class: Some(format!(
                    "debug-align debug-align-unmatched atom-{}",
                    a.index
                )),
            });
        }
    }

    for &(atom_idx, tag) in &dbg.tags {
        let Some(a) = by.get(&atom_idx) else {
            continue;
        };
        let color = tag_color(tag);
        let cx = a.x + dx;
        let cy = a.y + dy;
        out.push(Primitive::Circle {
            cx,
            cy,
            r,
            fill: Some(color.into()),
            stroke: Some("#ffffff".into()),
            stroke_width: 0.8,
            opacity: DEBUG_ALIGN_OPACITY,
            class: Some(format!(
                "debug-align debug-align-atom atom-{atom_idx} tag-{tag}"
            )),
        });
        out.push(Primitive::Text {
            x: cx,
            y: cy + font_px * 0.35,
            text: tag.to_string(),
            fill: "#ffffff".into(),
            font_size: font_px * 0.85,
            anchor: TextAnchor::Middle,
            class: Some(format!(
                "debug-align debug-align-tag atom-{atom_idx} tag-{tag}"
            )),
        });
    }

    let badge = match dbg.role {
        AlignDebugRole::Template => {
            let peers = dbg.peer_id.as_deref().unwrap_or("?");
            format!("align★T ← {peers} ({} atoms)", dbg.tags.len())
        }
        AlignDebugRole::Query => {
            let peer = dbg.peer_id.as_deref().unwrap_or("?");
            format!("align→Q → {peer} ({} atoms)", dbg.tags.len())
        }
    };
    out.push(Primitive::Text {
        x: 4.0,
        y: font_px * 1.1,
        text: badge,
        fill: DEBUG_ALIGN_BADGE.into(),
        font_size: font_px * 0.7,
        anchor: TextAnchor::Start,
        class: Some(format!(
            "debug-align debug-align-badge role-{}",
            match dbg.role {
                AlignDebugRole::Template => "template",
                AlignDebugRole::Query => "query",
            }
        )),
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::AlignDebugRole;

    #[test]
    fn query_tags_use_ref_indices() {
        let d = align_debug_query("m0", &[(2, 5), (3, 1)]);
        assert_eq!(d.role, AlignDebugRole::Query);
        assert_eq!(d.tags, vec![(2, 5), (3, 1)]);
    }

    #[test]
    fn template_tags_dedupe_ref_atoms() {
        let d = align_debug_template(
            &["a".into(), "b".into()],
            &[vec![(0, 1), (1, 2)], vec![(0, 1), (2, 3)]],
        );
        assert_eq!(d.role, AlignDebugRole::Template);
        assert_eq!(d.tags, vec![(1, 1), (2, 2), (3, 3)]);
        assert_eq!(d.peer_id.as_deref(), Some("a,b"));
    }
}
