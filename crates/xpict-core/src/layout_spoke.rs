//! Split CoordGen layout for ring-center spokes (CX ``m:``).
//!
//! 1. Detect spoke vs rest.
//! 2. Layout rest and each spoke **separately** (spoke graph has no dummy).
//! 3. Greedy stem from the ring centroid along a ring-bond bisector.
//! 4. 1-D refine (same hard no-cross / soft bisector energy).
//! 5. Write coords; chrome then treats the molecule as fixed.

use crate::align_layout::layout_or_align;
use crate::cxsmiles::cx_multicenters;
use crate::edge::AlignOpts;
use crate::layout_coordgen::{
    generate_2d_coordinates, CoordGenError, LayoutMolecule, LayoutResult,
};
use crate::scene::AlignDebug;
use crate::spokes::{
    bisector_angles, detect_ring_spokes, first_spoke_neighbor, greedy_spoke_angle, place_spoke,
};

/// Layout ``query``, placing CX ``m:`` ring-center spokes after the scaffold.
pub fn layout_or_align_with_spokes(
    query: &LayoutMolecule,
    query_atom_indices: &[i32],
    source: &str,
    reference: Option<&LayoutMolecule>,
    ref_atom_indices: Option<&[i32]>,
    atom_map: Option<&[(i32, i32)]>,
    peer_id: Option<&str>,
    align: Option<&AlignOpts>,
) -> Result<(LayoutResult, Option<AlignDebug>), CoordGenError> {
    let bonds: Vec<(i32, i32)> = query
        .bond_from
        .iter()
        .zip(query.bond_to.iter())
        .map(|(&a, &b)| (a, b))
        .collect();
    let groups = cx_multicenters(source);
    let comps = detect_ring_spokes(query.n_atoms(), &bonds, &groups);
    if comps.is_empty() {
        return layout_or_align(
            query,
            query_atom_indices,
            reference,
            ref_atom_indices,
            atom_map,
            peer_id,
            align,
        );
    }

    let spoke_atom_set: std::collections::HashSet<i32> = comps
        .iter()
        .flat_map(|c| c.atoms.iter().copied().chain(std::iter::once(c.dummy)))
        .collect();
    let rest_atoms: Vec<usize> = (0..query.n_atoms())
        .filter(|i| !spoke_atom_set.contains(&(*i as i32)))
        .collect();
    if rest_atoms.is_empty() {
        return layout_or_align(
            query,
            query_atom_indices,
            reference,
            ref_atom_indices,
            atom_map,
            peer_id,
            align,
        );
    }

    let (rest_layout, rest_from_sub) = subgraph(query, &rest_atoms);
    let rest_indices: Vec<i32> = rest_atoms
        .iter()
        .map(|&i| query_atom_indices.get(i).copied().unwrap_or(i as i32))
        .collect();
    let rest_map = atom_map.map(|pairs| {
        let rest_pub: std::collections::HashSet<i32> = rest_indices.iter().copied().collect();
        pairs
            .iter()
            .copied()
            .filter(|(q, _)| rest_pub.contains(q))
            .collect::<Vec<_>>()
    });
    let (rest_coords, dbg) = layout_or_align(
        &rest_layout,
        &rest_indices,
        reference,
        ref_atom_indices,
        rest_map.as_deref(),
        peer_id,
        align,
    )?;

    let n = query.n_atoms();
    let mut x = vec![0.0f32; n];
    let mut y = vec![0.0f32; n];
    for (sub, &old) in rest_from_sub.iter().zip(rest_atoms.iter()) {
        x[old] = rest_coords.x.get(*sub).copied().unwrap_or(0.0);
        y[old] = rest_coords.y.get(*sub).copied().unwrap_or(0.0);
    }

    let bond_len = mean_bond_len(&rest_coords, &rest_layout).max(0.5);
    let disk = 0.3 * bond_len;

    let mut taken_angles: Vec<f64> = Vec::new();
    // Larger substituents first so they claim the open bisector slots.
    let mut order: Vec<usize> = (0..comps.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(comps[i].atoms.len()));

    for &ci in &order {
        let comp = &comps[ci];
        let ring_xy: Vec<(i32, f64, f64)> = comp
            .endpts
            .iter()
            .filter_map(|&i| {
                let u = i as usize;
                (u < n).then_some((i, f64::from(x[u]), f64::from(y[u])))
            })
            .collect();
        if ring_xy.len() < 3 {
            continue;
        }
        let cx: f64 = ring_xy.iter().map(|p| p.1).sum::<f64>() / ring_xy.len() as f64;
        let cy: f64 = ring_xy.iter().map(|p| p.2).sum::<f64>() / ring_xy.len() as f64;
        let centroid = (cx, cy);
        let bis = bisector_angles(centroid, &ring_xy, &bonds);
        let theta = greedy_spoke_angle(
            centroid,
            f64::from(bond_len),
            &ring_xy,
            f64::from(disk),
            &bis,
            &taken_angles,
        );

        let dummy_u = comp.dummy as usize;
        if dummy_u < n {
            x[dummy_u] = cx as f32;
            y[dummy_u] = cy as f32;
        }

        let spoke_keep: Vec<usize> = comp.atoms.iter().map(|&a| a as usize).collect();
        if spoke_keep.is_empty() {
            taken_angles.push(theta);
            continue;
        }
        let (spoke_layout, _) = subgraph(query, &spoke_keep);
        let spoke_coords = if spoke_layout.n_atoms() == 0 {
            LayoutResult {
                x: Vec::new(),
                y: Vec::new(),
            }
        } else if spoke_layout.n_atoms() == 1 {
            LayoutResult {
                x: vec![0.0],
                y: vec![0.0],
            }
        } else {
            generate_2d_coordinates(&spoke_layout).unwrap_or(LayoutResult {
                x: vec![0.0; spoke_layout.n_atoms()],
                y: vec![0.0; spoke_layout.n_atoms()],
            })
        };

        let local: Vec<(f64, f64)> = (0..spoke_keep.len())
            .map(|i| {
                (
                    f64::from(spoke_coords.x.get(i).copied().unwrap_or(0.0)),
                    f64::from(spoke_coords.y.get(i).copied().unwrap_or(0.0)),
                )
            })
            .collect();
        let attach_idx = spoke_keep
            .iter()
            .position(|&u| u as i32 == comp.atom)
            .unwrap_or(0);
        let neighbor = first_spoke_neighbor(&bonds, comp.atom, &comp.atoms, comp.dummy);
        let neighbor_idx = neighbor.and_then(|nb| spoke_keep.iter().position(|&u| u as i32 == nb));
        let placed = place_spoke(
            &local,
            attach_idx,
            neighbor_idx,
            centroid,
            theta,
            f64::from(bond_len),
        );
        for (i, &old) in spoke_keep.iter().enumerate() {
            if let Some(&(px, py)) = placed.get(i) {
                x[old] = px as f32;
                y[old] = py as f32;
            }
        }
        taken_angles.push(theta);
    }

    Ok((LayoutResult { x, y }, dbg))
}

fn mean_bond_len(coords: &LayoutResult, layout: &LayoutMolecule) -> f32 {
    let mut lens = Vec::new();
    for (&a, &b) in layout.bond_from.iter().zip(layout.bond_to.iter()) {
        let i = a as usize;
        let j = b as usize;
        if i < coords.x.len() && j < coords.x.len() {
            let dx = coords.x[i] - coords.x[j];
            let dy = coords.y[i] - coords.y[j];
            lens.push((dx * dx + dy * dy).sqrt());
        }
    }
    if lens.is_empty() {
        1.5
    } else {
        lens.iter().sum::<f32>() / lens.len() as f32
    }
}

/// Subgraph on ``keep`` (dense indices into ``src``). Returns (sub, sub_index→keep_pos).
fn subgraph(src: &LayoutMolecule, keep: &[usize]) -> (LayoutMolecule, Vec<usize>) {
    let n = src.n_atoms();
    let mut map = vec![None; n];
    for (new, &old) in keep.iter().enumerate() {
        if old < n {
            map[old] = Some(new as i32);
        }
    }
    let mut out = LayoutMolecule {
        atomic_numbers: keep
            .iter()
            .map(|&i| src.atomic_numbers.get(i).copied().unwrap_or(6))
            .collect(),
        formal_charges: keep
            .iter()
            .map(|&i| src.formal_charges.get(i).copied().unwrap_or(0))
            .collect(),
        ..Default::default()
    };
    for (k, (&a, &b)) in src.bond_from.iter().zip(&src.bond_to).enumerate() {
        let (Some(na), Some(nb)) = (
            map.get(a as usize).copied().flatten(),
            map.get(b as usize).copied().flatten(),
        ) else {
            continue;
        };
        out.bond_from.push(na);
        out.bond_to.push(nb);
        out.bond_orders
            .push(src.bond_orders.get(k).copied().unwrap_or(1));
        out.bond_stereos
            .push(src.bond_stereos.get(k).copied().unwrap_or(0));
    }
    let id = (0..keep.len()).collect();
    (out, id)
}
