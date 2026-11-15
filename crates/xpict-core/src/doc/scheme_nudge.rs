//! Soft mol translation after packing: open a clear shaft corridor when a
//! small move helps, without packing mols into each other.

use std::collections::{HashMap, HashSet};

use crate::engine_config::EngineConfig;

use super::scheme_layout::SCHEME_INK_STANDOFF;
use super::{DepictSpec, EdgeNode, LayoutDirection, LayoutOpts};

type Pt = (f64, f64);

/// Align multi-endpoint groups so opposite sides share a cross-axis midpoint.
///
/// ELK often parks the singleton (or the other stack) near the top of a
/// co-endpoint column. Default packing should line up the midpoints of the
/// left/right (or top/bottom) group boxes — not their tops — unless label
/// text is being aligned deliberately.
pub fn align_co_endpoint_group_mids(
    spec: &DepictSpec,
    positions: &HashMap<String, Pt>,
    sizes: &[(String, f64, f64)],
    layout: &LayoutOpts,
) -> HashMap<String, Pt> {
    let size_of: HashMap<&str, (f64, f64)> = sizes
        .iter()
        .map(|(id, w, h)| (id.as_str(), (*w, *h)))
        .collect();
    let mut pos = positions.clone();
    let vertical_flow = matches!(
        layout.direction_or_default(),
        LayoutDirection::Up | LayoutDirection::Down
    );
    for edge in spec.edges() {
        if !edge.participates_in_layout() {
            continue;
        }
        let sources = edge.sources.as_slice();
        let targets = edge.targets.as_slice();
        if sources.len() < 2 && targets.len() < 2 {
            continue;
        }
        let Some(src_mid) = group_cross_mid(sources, &pos, &size_of, vertical_flow) else {
            continue;
        };
        let Some(tgt_mid) = group_cross_mid(targets, &pos, &size_of, vertical_flow) else {
            continue;
        };
        let delta = src_mid - tgt_mid;
        if delta.abs() < 0.5 {
            continue;
        }
        // Keep the multi-mol stack planted; slide the other side onto its mid.
        // When both sides are multi, slide targets onto the source mid.
        let move_targets = sources.len() >= 2;
        let ids = if move_targets { targets } else { sources };
        let signed = if move_targets { delta } else { -delta };
        for id in ids {
            if let Some(p) = pos.get_mut(id) {
                if vertical_flow {
                    p.0 += signed;
                } else {
                    p.1 += signed;
                }
            }
        }
    }
    pos
}

fn group_cross_mid(
    ids: &[String],
    pos: &HashMap<String, Pt>,
    size_of: &HashMap<&str, (f64, f64)>,
    vertical_flow: bool,
) -> Option<f64> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut any = false;
    for id in ids {
        let Some((x0, y0, x1, y1)) = mol_aabb(pos, size_of, id) else {
            continue;
        };
        if vertical_flow {
            lo = lo.min(x0);
            hi = hi.max(x1);
        } else {
            lo = lo.min(y0);
            hi = hi.max(y1);
        }
        any = true;
    }
    any.then_some(0.5 * (lo + hi))
}

/// Propose small translations that turn blocked centroid chords into clear ones.
///
/// Only moves **blockers** (mols whose box crosses a chord interior), never the
/// edge endpoints. Rejects any candidate that would overlap another mol box.
pub fn propose_mol_nudges(
    spec: &DepictSpec,
    positions: &HashMap<String, Pt>,
    sizes: &[(String, f64, f64)],
    engine: &EngineConfig,
) -> HashMap<String, Pt> {
    let nudge = &engine.nudge;
    let mut pos = positions.clone();
    if !nudge.enabled {
        return pos;
    }
    let mol_gap = SCHEME_INK_STANDOFF + nudge.mol_gap_extra;
    let size_of: HashMap<&str, (f64, f64)> = sizes
        .iter()
        .map(|(id, w, h)| (id.as_str(), (*w, *h)))
        .collect();
    let edges = spec.edges();
    if edges.is_empty() || size_of.len() < 3 {
        return pos;
    }

    // Greedy: unlock one blocked chord at a time with the cheapest safe nudge.
    for _pass in 0..size_of.len().min(nudge.max_passes) {
        let Some((blocker, dx, dy)) =
            best_nudge_for_any_blocked(&edges, &pos, &size_of, nudge, mol_gap)
        else {
            break;
        };
        if let Some(p) = pos.get_mut(&blocker) {
            p.0 += dx;
            p.1 += dy;
        }
    }
    pos
}

fn best_nudge_for_any_blocked(
    edges: &[&EdgeNode],
    pos: &HashMap<String, Pt>,
    size_of: &HashMap<&str, (f64, f64)>,
    nudge: &crate::engine_config::NudgeConfig,
    mol_gap: f64,
) -> Option<(String, f64, f64)> {
    let mut best: Option<(f64, String, f64, f64)> = None; // (cost, id, dx, dy)
    for edge in edges {
        let (src, tgt) = edge_facing_src_tgt(edge, pos, size_of);
        if src.is_empty() || tgt.is_empty() || src == tgt {
            continue;
        }
        let a = box_center(pos, size_of, &src)?;
        let b = box_center(pos, size_of, &tgt)?;
        // Sibling co-endpoints are intentional neighbors (+), not blockers.
        let mut owned: HashSet<String> = HashSet::new();
        for id in edge.sources.as_slice().iter().chain(edge.targets.as_slice()) {
            owned.insert(id.clone());
        }
        let blockers = chord_blockers_except(pos, size_of, a, b, &owned);
        if blockers.is_empty() {
            continue;
        }
        let (px, py) = unit_perp(a, b);
        for blocker in blockers {
            for &step in &nudge.steps {
                if step > nudge.max_nudge + 1e-9 {
                    continue;
                }
                for sign in [1.0_f64, -1.0] {
                    let dx = px * step * sign;
                    let dy = py * step * sign;
                    let mut trial = pos.clone();
                    if let Some(p) = trial.get_mut(&blocker) {
                        p.0 += dx;
                        p.1 += dy;
                    } else {
                        continue;
                    }
                    if boxes_overlap_any(&trial, size_of, mol_gap) {
                        continue;
                    }
                    // Must clear *this* chord; do not create new overlaps.
                    if !chord_clear_except(&trial, size_of, a, b, &owned) {
                        continue;
                    }
                    let cost = step; // prefer smaller moves
                    let better = best
                        .as_ref()
                        .map(|(c, ..)| cost + 1e-6 < *c)
                        .unwrap_or(true);
                    if better {
                        best = Some((cost, blocker.clone(), dx, dy));
                    }
                }
            }
        }
    }
    best.map(|(_, id, dx, dy)| (id, dx, dy))
}

fn chord_blockers_except(
    pos: &HashMap<String, Pt>,
    size_of: &HashMap<&str, (f64, f64)>,
    a: Pt,
    b: Pt,
    owned: &HashSet<String>,
) -> Vec<String> {
    let mut out = Vec::new();
    for &id in size_of.keys() {
        if owned.contains(id) {
            continue;
        }
        let Some(aabb) = mol_aabb(pos, size_of, id) else {
            continue;
        };
        if segment_hits_aabb(a, b, aabb, SCHEME_INK_STANDOFF) {
            out.push(id.to_string());
        }
    }
    out
}

fn chord_clear_except(
    pos: &HashMap<String, Pt>,
    size_of: &HashMap<&str, (f64, f64)>,
    a: Pt,
    b: Pt,
    owned: &HashSet<String>,
) -> bool {
    chord_blockers_except(pos, size_of, a, b, owned).is_empty()
}

fn boxes_overlap_any(
    pos: &HashMap<String, Pt>,
    size_of: &HashMap<&str, (f64, f64)>,
    mol_gap: f64,
) -> bool {
    let ids: Vec<&str> = size_of.keys().copied().collect();
    for i in 0..ids.len() {
        let Some(ai) = mol_aabb(pos, size_of, ids[i]) else {
            continue;
        };
        for j in (i + 1)..ids.len() {
            let Some(aj) = mol_aabb(pos, size_of, ids[j]) else {
                continue;
            };
            if aabb_overlap_gap(ai, aj, mol_gap) {
                return true;
            }
        }
    }
    false
}

fn aabb_overlap_gap(
    a: (f64, f64, f64, f64),
    b: (f64, f64, f64, f64),
    gap: f64,
) -> bool {
    let (ax0, ay0, ax1, ay1) = a;
    let (bx0, by0, bx1, by1) = b;
    ax0 < bx1 + gap && ax1 + gap > bx0 && ay0 < by1 + gap && ay1 + gap > by0
}

fn mol_aabb(
    pos: &HashMap<String, Pt>,
    size_of: &HashMap<&str, (f64, f64)>,
    id: &str,
) -> Option<(f64, f64, f64, f64)> {
    let (x, y) = *pos.get(id)?;
    let (w, h) = *size_of.get(id)?;
    Some((x, y, x + w, y + h))
}

fn box_center(
    pos: &HashMap<String, Pt>,
    size_of: &HashMap<&str, (f64, f64)>,
    id: &str,
) -> Option<Pt> {
    let (x0, y0, x1, y1) = mol_aabb(pos, size_of, id)?;
    Some((0.5 * (x0 + x1), 0.5 * (y0 + y1)))
}

fn segment_hits_aabb(a: Pt, b: Pt, aabb: (f64, f64, f64, f64), pad: f64) -> bool {
    let (x0, y0, x1, y1) = aabb;
    let x0 = x0 - pad;
    let y0 = y0 - pad;
    let x1 = x1 + pad;
    let y1 = y1 + pad;
    // Sample the open segment (skip endpoints near terminals).
    for i in 1..8 {
        let t = i as f64 / 8.0;
        let p = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        if p.0 >= x0 && p.0 <= x1 && p.1 >= y0 && p.1 <= y1 {
            return true;
        }
    }
    false
}

fn unit_perp(a: Pt, b: Pt) -> Pt {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let l = dx.hypot(dy).max(1e-9);
    (-dy / l, dx / l)
}

fn edge_src_tgt(edge: &EdgeNode) -> (String, String) {
    let src = edge.sources.as_slice().first().cloned().unwrap_or_default();
    let tgt = edge.targets.as_slice().first().cloned().unwrap_or_default();
    (src, tgt)
}

/// Near-side source/target for multi-endpoint edges (same rule as scheme_route).
fn edge_facing_src_tgt(
    edge: &EdgeNode,
    pos: &HashMap<String, Pt>,
    size_of: &HashMap<&str, (f64, f64)>,
) -> (String, String) {
    let sources = edge.sources.as_slice();
    let targets = edge.targets.as_slice();
    if sources.is_empty() || targets.is_empty() {
        return (String::new(), String::new());
    }
    if sources.len() == 1 && targets.len() == 1 {
        return (sources[0].clone(), targets[0].clone());
    }
    let center = |id: &str| -> Option<Pt> { box_center(pos, size_of, id) };
    let mean = |ids: &[String]| -> Option<Pt> {
        let mut sx = 0.0;
        let mut sy = 0.0;
        let mut n = 0.0;
        for id in ids {
            let Some((x, y)) = center(id) else {
                continue;
            };
            sx += x;
            sy += y;
            n += 1.0;
        }
        (n >= 1.0).then_some((sx / n, sy / n))
    };
    let Some(src_g) = mean(sources) else {
        return edge_src_tgt(edge);
    };
    let Some(tgt_g) = mean(targets) else {
        return edge_src_tgt(edge);
    };
    let closest = |ids: &[String], to: Pt| -> String {
        ids.iter()
            .min_by(|a, b| {
                let da = center(a)
                    .map(|c| (c.0 - to.0).hypot(c.1 - to.1))
                    .unwrap_or(f64::INFINITY);
                let db = center(b)
                    .map(|c| (c.0 - to.0).hypot(c.1 - to.1))
                    .unwrap_or(f64::INFINITY);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
            .unwrap_or_default()
    };
    (closest(sources, tgt_g), closest(targets, src_g))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::spec::{
        DepictSpec, EdgeArrow, EdgeNode, EdgeNodeKind, MolIds, MolNode, Node,
    };
    use crate::metrics::BOND_PX;

    fn mol(id: &str) -> Node {
        Node::Mol(MolNode {
            id: Some(id.into()),
            smiles: Some("C".into()),
            ..Default::default()
        })
    }

    fn edge(s: &str, t: &str) -> Node {
        Node::Edge(EdgeNode {
            type_: EdgeNodeKind::Edge,
            sources: MolIds::One(s.into()),
            targets: MolIds::One(t.into()),
            label: None,
            edge_routing: None,
            layout: None,
            role: None,
            arrow: EdgeArrow::Forward,
            color: None,
            stroke_width: None,
            dashed: false,
        })
    }

    fn scheme(children: Vec<Node>) -> DepictSpec {
        DepictSpec::ReactionScheme {
            id: None,
            children,
            layout: None,
            opts: None,
            color: None,
            scale: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
        svg: None,
        }
    }

    #[test]
    fn nudges_middle_blocker_off_the_chord() {
        let spec = scheme(vec![mol("A"), mol("B"), mol("C"), edge("A", "C")]);
        let positions = HashMap::from([
            ("A".into(), (0.0, 0.0)),
            ("B".into(), (100.0, 0.0)),
            ("C".into(), (200.0, 0.0)),
        ]);
        let sizes = vec![
            ("A".into(), 80.0, 80.0),
            ("B".into(), 80.0, 80.0),
            ("C".into(), 80.0, 80.0),
        ];
        let mut eng = EngineConfig::pretty();
        eng.nudge = crate::engine_config::NudgeConfig::on();
        let nudged = propose_mol_nudges(&spec, &positions, &sizes, &eng);
        let b0 = positions["B"];
        let b1 = nudged["B"];
        let dy = (b1.1 - b0.1).abs();
        assert!(
            dy >= 0.5 * BOND_PX - 1e-6,
            "blocker B should move off the A–C chord: {b0:?} → {b1:?}"
        );
        assert!(
            (b1.0 - b0.0).abs() < 1e-6,
            "prefer pure perpendicular: {b0:?} → {b1:?}"
        );
        // A and C stay put.
        assert_eq!(nudged["A"], positions["A"]);
        assert_eq!(nudged["C"], positions["C"]);
    }

    #[test]
    fn disabled_nudge_leaves_positions_unchanged() {
        let spec = scheme(vec![mol("A"), mol("B"), mol("C"), edge("A", "C")]);
        let positions = HashMap::from([
            ("A".into(), (0.0, 0.0)),
            ("B".into(), (100.0, 0.0)),
            ("C".into(), (200.0, 0.0)),
        ]);
        let sizes = vec![
            ("A".into(), 80.0, 80.0),
            ("B".into(), 80.0, 80.0),
            ("C".into(), 80.0, 80.0),
        ];
        let eng = EngineConfig::pretty();
        assert!(!eng.nudge.enabled);
        let nudged = propose_mol_nudges(&spec, &positions, &sizes, &eng);
        assert_eq!(nudged, positions);
    }

    #[test]
    fn refuses_nudge_that_would_overlap() {
        // B is blocked on the chord but packed against another mol above —
        // only room downward; we still accept a safe direction. Pack *both*
        // sides so every candidate overlaps.
        let spec = scheme(vec![
            mol("A"),
            mol("B"),
            mol("C"),
            mol("U"),
            mol("D"),
            edge("A", "C"),
        ]);
        let mut eng = EngineConfig::pretty();
        eng.nudge = crate::engine_config::NudgeConfig::on();
        let mol_gap = SCHEME_INK_STANDOFF + eng.nudge.mol_gap_extra;
        let positions = HashMap::from([
            ("A".into(), (0.0, 100.0)),
            ("B".into(), (100.0, 100.0)),
            ("C".into(), (200.0, 100.0)),
            ("U".into(), (100.0, 100.0 - 80.0 - mol_gap + 1.0)),
            ("D".into(), (100.0, 100.0 + 80.0 + mol_gap - 1.0)),
        ]);
        let sizes = vec![
            ("A".into(), 80.0, 80.0),
            ("B".into(), 80.0, 80.0),
            ("C".into(), 80.0, 80.0),
            ("U".into(), 80.0, 80.0),
            ("D".into(), 80.0, 80.0),
        ];
        let nudged = propose_mol_nudges(&spec, &positions, &sizes, &eng);
        assert_eq!(
            nudged["B"], positions["B"],
            "must not nudge into U/D: {:?} vs {:?}",
            nudged["B"],
            positions["B"]
        );
    }
}
