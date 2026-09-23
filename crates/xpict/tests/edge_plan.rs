//! EdgePlan processor tests (native RDKit host).
//!
//! Plan validation / forest shape / CX chrome: ``xpict-core``.

use xpict::{build_align_plan, process_edge_plan};
use xpict_core::edge::{CoordMethod, EdgeTaskResult};

#[test]
fn edge_plan_atom_map_align() {
    let plan = build_align_plan(
        "c1ccccc1",
        "Cc1ccccc1",
        Some(vec![(1, 0), (2, 1), (3, 2), (4, 3), (5, 4), (6, 5)]),
    );
    let result = process_edge_plan(&plan).expect("process");
    let EdgeTaskResult::CoordGen { ok, molecules } = &result.results[0];
    assert!(*ok);
    assert_eq!(molecules.len(), 2);
    assert_eq!(molecules[0].method, CoordMethod::Free);
    assert!(molecules[0].ok);
    assert_eq!(molecules[1].method, CoordMethod::AtomMap);
    assert!(molecules[1].ok);
    assert_eq!(molecules[1].molecule.as_ref().unwrap().atoms.len(), 7);
}

#[test]
fn edge_plan_mcs_align() {
    let plan = build_align_plan("c1ccc(O)cc1", "O=C1C=CC(=O)C=C1", None);
    let result = process_edge_plan(&plan).expect("process");
    let EdgeTaskResult::CoordGen { molecules, .. } = &result.results[0];
    assert!(molecules[1].ok);
    assert_eq!(molecules[1].method, CoordMethod::Mcs);
}

#[test]
fn edge_plan_align_failure_falls_back() {
    let plan = build_align_plan("O=C1C=CC(=O)C=C1", "C1CCCCC1CCOCCCCCC", None);
    let result = process_edge_plan(&plan).expect("process");
    let EdgeTaskResult::CoordGen { ok, molecules } = &result.results[0];
    assert!(*ok, "task ok when all have coords");
    assert_eq!(molecules.len(), 2);
    assert!(molecules[0].ok);
    assert!(molecules[1].ok);
    assert_eq!(molecules[1].method, CoordMethod::None);
    assert!(molecules[1].molecule.is_some());
    assert!(molecules[1]
        .error
        .as_deref()
        .unwrap_or("")
        .contains("fell back"));
}
