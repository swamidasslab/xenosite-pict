//! Constrained CoordGen align: cost-map then soft-constrain query onto ref.

use crate::align_cost::{
    cost_mapping, AtomChirality, AtomFeat, BondFeat, EzStereo, MappingResult, MolGraph,
};
use crate::layout_coordgen::{
    generate_2d_coordinates, CoordGenError, LayoutMolecule, LayoutResult, BOND_STEREO_CIS,
    BOND_STEREO_NONE, BOND_STEREO_TRANS,
};

/// Build a [`MolGraph`] from CoordGen-style flat tables.
pub fn mol_graph_from_layout(mol: &LayoutMolecule) -> MolGraph {
    let atoms = mol
        .atomic_numbers
        .iter()
        .zip(mol.formal_charges.iter())
        .map(|(&z, &ch)| AtomFeat {
            atomic_number: z.clamp(0, 255) as u8,
            formal_charge: ch.clamp(i8::MIN as i32, i8::MAX as i32) as i8,
            radical: 0,
            chirality: AtomChirality::None,
        })
        .collect();
    let bonds = mol
        .bond_from
        .iter()
        .enumerate()
        .map(|(i, &a)| {
            let b = mol.bond_to[i];
            // CTAB / Chematic aromatic is order 4 — do not clamp to 3 (triple).
            let raw = mol.bond_orders.get(i).copied().unwrap_or(1);
            let aromatic = raw >= 4;
            let order = if aromatic { 1 } else { raw.clamp(0, 3) as u8 };
            let stereo = match mol.bond_stereos.get(i).copied().unwrap_or(BOND_STEREO_NONE) {
                x if x == BOND_STEREO_CIS => EzStereo::Cis,
                x if x == BOND_STEREO_TRANS => EzStereo::Trans,
                _ => EzStereo::None,
            };
            let (s1, s2) = match (&mol.bond_stereo_atom1, &mol.bond_stereo_atom2) {
                (Some(a1), Some(a2)) if a1[i] >= 0 && a2[i] >= 0 => {
                    (Some(a1[i] as u16), Some(a2[i] as u16))
                }
                _ => (None, None),
            };
            BondFeat {
                begin: a as u16,
                end: b as u16,
                order,
                aromatic,
                stereo,
                stereo_atom1: s1,
                stereo_atom2: s2,
            }
        })
        .collect();
    MolGraph::new(atoms, bonds)
}

#[derive(Debug, Clone)]
pub struct ConstrainedAlignResult {
    pub mapping: MappingResult,
    pub ref_coords: LayoutResult,
    pub query_coords: LayoutResult,
}

fn ensure_ref_coords(mol: &LayoutMolecule) -> Result<LayoutResult, CoordGenError> {
    if let (Some(x), Some(y)) = (&mol.input_x, &mol.input_y) {
        if x.len() == mol.n_atoms() && y.len() == mol.n_atoms() {
            return Ok(LayoutResult {
                x: x.clone(),
                y: y.clone(),
            });
        }
    }
    let mut free = mol.clone();
    free.has_template_match = None;
    free.is_atom_fixed = None;
    free.input_x = None;
    free.input_y = None;
    generate_2d_coordinates(&free)
}

/// Cost-map `query` onto `reference`, free-layout ref if needed, then layout
/// query with matched atoms **constrained** (soft) to ref coordinates.
pub fn align_molecule_constrained(
    reference: &LayoutMolecule,
    query: &LayoutMolecule,
) -> Result<ConstrainedAlignResult, CoordGenError> {
    align_molecule_constrained_with_map(reference, query, None)
}

/// Like [`align_molecule_constrained`], but ``dense_map`` (query→ref dense
/// indices) skips MCS when provided.
pub fn align_molecule_constrained_with_map(
    reference: &LayoutMolecule,
    query: &LayoutMolecule,
    dense_map: Option<&[(u16, u16)]>,
) -> Result<ConstrainedAlignResult, CoordGenError> {
    let mapping = if let Some(pairs) = dense_map {
        let mut map = std::collections::HashMap::new();
        for &(q, r) in pairs {
            map.insert(q, r);
        }
        MappingResult { map, cost: 0 }
    } else {
        let ref_g = mol_graph_from_layout(reference);
        let query_g = mol_graph_from_layout(query);
        cost_mapping(&ref_g, &query_g).ok_or(CoordGenError::Empty)?
    };

    let ref_coords = ensure_ref_coords(reference)?;

    let nq = query.n_atoms();
    let mut has_template = vec![0i32; nq];
    let mut input_x = vec![0.0f32; nq];
    let mut input_y = vec![0.0f32; nq];
    for (&qi, &ri) in &mapping.map {
        let qi = qi as usize;
        let ri = ri as usize;
        if qi < nq && ri < ref_coords.x.len() {
            has_template[qi] = 1;
            input_x[qi] = ref_coords.x[ri];
            input_y[qi] = ref_coords.y[ri];
        }
    }

    let mut q_lay = query.clone();
    q_lay.has_template_match = Some(has_template);
    q_lay.is_atom_fixed = None;
    q_lay.input_x = Some(input_x);
    q_lay.input_y = Some(input_y);
    let query_coords = generate_2d_coordinates(&q_lay)?;

    Ok(ConstrainedAlignResult {
        mapping,
        ref_coords,
        query_coords,
    })
}

/// Free-layout ``query``, or constrained-align onto ``reference`` when given.
///
/// ``atom_map`` is **atom-index** pairs ``(query_atom, ref_atom)``; dense indices
/// are derived from ``query_atom_indices`` / ``ref_atom_indices`` (parallel to
/// the LayoutMolecule atom tables). When omitted, MCS supplies the map.
pub fn layout_or_align(
    query: &LayoutMolecule,
    query_atom_indices: &[i32],
    reference: Option<&LayoutMolecule>,
    ref_atom_indices: Option<&[i32]>,
    atom_map: Option<&[(i32, i32)]>,
    peer_id: Option<&str>,
) -> Result<(LayoutResult, Option<crate::scene::AlignDebug>), CoordGenError> {
    use crate::debug_align::align_debug_query;

    let Some(reference) = reference else {
        let coords = generate_2d_coordinates(query)?;
        return Ok((coords, None));
    };

    let ref_idx = ref_atom_indices.unwrap_or(&[]);
    let dense = if let Some(pairs) = atom_map {
        let q_of: std::collections::HashMap<i32, u16> = query_atom_indices
            .iter()
            .enumerate()
            .map(|(i, &a)| (a, i as u16))
            .collect();
        let r_of: std::collections::HashMap<i32, u16> = ref_idx
            .iter()
            .enumerate()
            .map(|(i, &a)| (a, i as u16))
            .collect();
        let mut dense_pairs = Vec::new();
        for &(qa, ra) in pairs {
            if let (Some(&qi), Some(&ri)) = (q_of.get(&qa), r_of.get(&ra)) {
                dense_pairs.push((qi, ri));
            }
        }
        if dense_pairs.is_empty() {
            return Err(CoordGenError::Empty);
        }
        Some(dense_pairs)
    } else {
        None
    };

    let res = align_molecule_constrained_with_map(reference, query, dense.as_deref())?;

    let mut tags: Vec<(i32, i32)> = res
        .mapping
        .map
        .iter()
        .filter_map(|(&qi, &ri)| {
            let qa = *query_atom_indices.get(qi as usize)?;
            let ra = *ref_idx.get(ri as usize)?;
            Some((qa, ra))
        })
        .collect();
    tags.sort_by_key(|(a, _)| *a);
    let dbg = align_debug_query(peer_id.unwrap_or("?"), &tags);
    Ok((res.query_coords, Some(dbg)))
}

/// Regular-hexagon fit error for a closed ring of coordinates (max vertex
/// distance to the best-fit regular hexagon through Kabsch-like centering).
pub fn hexagon_fit_max_dev(xy: &[(f64, f64)]) -> f64 {
    let n = xy.len();
    if n < 3 {
        return f64::INFINITY;
    }
    let cx = xy.iter().map(|p| p.0).sum::<f64>() / n as f64;
    let cy = xy.iter().map(|p| p.1).sum::<f64>() / n as f64;
    let radii: Vec<f64> = xy.iter().map(|(x, y)| (x - cx).hypot(y - cy)).collect();
    let r = radii.iter().sum::<f64>() / n as f64;
    if r < 1e-9 {
        return f64::INFINITY;
    }
    // Angle of first point; assume equal spacing.
    let a0 = (xy[0].1 - cy).atan2(xy[0].0 - cx);
    let mut max_dev = 0.0f64;
    for (i, &(x, y)) in xy.iter().enumerate() {
        let ang = a0 + (i as f64) * (std::f64::consts::TAU / n as f64);
        let tx = cx + r * ang.cos();
        let ty = cy + r * ang.sin();
        max_dev = max_dev.max((x - tx).hypot(y - ty));
    }
    // Also try reverse winding.
    let mut max_rev = 0.0f64;
    for (i, &(x, y)) in xy.iter().enumerate() {
        let ang = a0 - (i as f64) * (std::f64::consts::TAU / n as f64);
        let tx = cx + r * ang.cos();
        let ty = cy + r * ang.sin();
        max_rev = max_rev.max((x - tx).hypot(y - ty));
    }
    max_dev.min(max_rev) / r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_coordgen::LayoutMolecule;

    fn chain6() -> LayoutMolecule {
        LayoutMolecule {
            atomic_numbers: vec![6; 6],
            formal_charges: vec![0; 6],
            bond_from: vec![0, 1, 2, 3, 4],
            bond_to: vec![1, 2, 3, 4, 5],
            bond_orders: vec![1; 5],
            bond_stereos: vec![0; 5],
            ..Default::default()
        }
    }

    fn cyclo6() -> LayoutMolecule {
        LayoutMolecule {
            atomic_numbers: vec![6; 6],
            formal_charges: vec![0; 6],
            bond_from: vec![0, 1, 2, 3, 4, 0],
            bond_to: vec![1, 2, 3, 4, 5, 5],
            bond_orders: vec![1; 6],
            bond_stereos: vec![0; 6],
            ..Default::default()
        }
    }

    #[test]
    fn hexane_cyclohexane_full_map_hexagon() {
        let hexane = chain6();
        let cyclo = cyclo6();
        let res = align_molecule_constrained(&hexane, &cyclo).expect("align");
        assert_eq!(res.mapping.map.len(), 6, "map={:?}", res.mapping.map);

        // Ref hexane should be elongated (chain), not ring-like.
        let rx: Vec<(f64, f64)> = res
            .ref_coords
            .x
            .iter()
            .zip(res.ref_coords.y.iter())
            .map(|(&x, &y)| (x as f64, y as f64))
            .collect();
        let ref_span = {
            let xs: Vec<_> = rx.iter().map(|p| p.0).collect();
            let ys: Vec<_> = rx.iter().map(|p| p.1).collect();
            let dx = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                - xs.iter().cloned().fold(f64::INFINITY, f64::min);
            let dy = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                - ys.iter().cloned().fold(f64::INFINITY, f64::min);
            dx.max(dy)
        };
        assert!(
            ref_span > 80.0,
            "hexane should be chain-like, span={ref_span}"
        );

        let qxy: Vec<(f64, f64)> = res
            .query_coords
            .x
            .iter()
            .zip(res.query_coords.y.iter())
            .map(|(&x, &y)| (x as f64, y as f64))
            .collect();
        // Soft-constrained cyclohexane must stay near-regular, not flatten to the chain.
        let dev = hexagon_fit_max_dev(&qxy);
        assert!(
            dev < 0.35,
            "constrained cyclohexane not hexagonal enough: rel_dev={dev}"
        );
    }
}
