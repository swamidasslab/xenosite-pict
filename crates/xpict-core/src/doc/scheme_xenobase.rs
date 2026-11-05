//! Xenobase network fixtures for visibility routing quality + timing.

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use crate::doc::depict;
    use crate::doc::review_regress::shaft_interior_clear_of_mols;
    use crate::doc::spec::DepictSpec;

    const EPIRUBICIN: &str =
        include_str!("../../tests/fixtures/xenobase/epirubicin.json");
    const LIDOCAINE: &str = include_str!("../../tests/fixtures/xenobase/lidocaine.json");
    const RANOLAZINE: &str =
        include_str!("../../tests/fixtures/xenobase/ranolazine.json");

    fn render_fixture(json: &str) -> crate::scene::Scene {
        let spec: DepictSpec = serde_json::from_str(json).expect("xenobase DepictSpec");
        depict(&spec).expect("depict").remove(0).scene
    }


    #[test]
    fn epirubicin_shafts_clear_chem() {
        let scene = render_fixture(EPIRUBICIN);
        assert!(
            scene.viewports.len() >= 6,
            "expected multiple mols, got {}",
            scene.viewports.len()
        );
        shaft_interior_clear_of_mols(&scene, -1.5);
    }

    #[test]
    fn lidocaine_shafts_clear_chem() {
        let scene = render_fixture(LIDOCAINE);
        assert!(
            scene.viewports.len() >= 10,
            "expected ~12 mols, got {}",
            scene.viewports.len()
        );
        shaft_interior_clear_of_mols(&scene, -1.5);
    }

    #[test]
    // Expected failure until dense (~40-mol) visibility + reopt is sped up.
    // should_panic (not #[ignore]): unexpected under-budget passes fail CI.
    #[should_panic(expected = "ranolazine compose took")]
    fn ranolazine_compose_under_budget() {
        let t0 = Instant::now();
        let scene = render_fixture(RANOLAZINE);
        let ms = t0.elapsed().as_millis();
        assert!(
            scene.viewports.len() >= 30,
            "large net should place dozens of mols, got {}",
            scene.viewports.len()
        );
        assert!(
            !scene.overlays.is_empty(),
            "large net must paint scheme edges"
        );
        // Debug builds spend most of this in CoordGen/paint for ~40 mols; the
        // visibility router itself must not reintroduce the old NM freeze.
        // Release CI is the tight gate once this should_panic is removed.
        let budget = if cfg!(debug_assertions) { 20_000 } else { 2_000 };
        assert!(
            ms < budget,
            "ranolazine compose took {ms}ms (budget {budget}ms)"
        );
    }
}
