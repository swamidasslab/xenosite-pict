//! Xenobase network fixtures for visibility routing quality + timing.

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use crate::doc::depict;
    use crate::doc::review_regress::{
        shaft_interior_clear_of_mols, shaft_interior_clear_of_viewport_boxes,
    };
    use crate::doc::spec::DepictSpec;

    const EPIRUBICIN: &str =
        include_str!("../../tests/fixtures/xenobase/epirubicin.json");
    const LIDOCAINE: &str = include_str!("../../tests/fixtures/xenobase/lidocaine.json");
    const RANOLAZINE: &str =
        include_str!("../../tests/fixtures/xenobase/ranolazine.json");
    /// xbn:0004961 Morphine sulfate with Intermediates on (live xenobase dump).
    const MORPHINE: &str =
        include_str!("../../tests/fixtures/xenobase/morphine-sulfate.json");

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
        // visibility router itself is ~1s release after sparse A* routing.
        let budget = if cfg!(debug_assertions) { 20_000 } else { 2_000 };
        assert!(
            ms < budget,
            "ranolazine compose took {ms}ms (budget {budget}ms)"
        );
    }

    #[test]
    fn morphine_sulfate_shafts_clear_chem() {
        // Dense fan from the Morphine root; shafts must not cut through
        // sibling metabolite boxes (esp. Morphine-3-sulfate / N-oxide fan).
        let scene = render_fixture(MORPHINE);
        assert!(
            scene.viewports.len() >= 16,
            "expected ~17 mols, got {}",
            scene.viewports.len()
        );
        shaft_interior_clear_of_mols(&scene, -1.5);
    }

    #[test]
    fn morphine_sulfate_shafts_clear_viewport_boxes() {
        // Whitespace inside a mol tile still reads as "arrow through molecule"
        // on this fan — require clearance of foreign painted viewports.
        let scene = render_fixture(MORPHINE);
        shaft_interior_clear_of_viewport_boxes(&scene, 4.0);
    }

    #[test]
    fn morphine_sulfate_shafts_have_length() {
        // Tip hugging must leave a real tip→tip shaft (AABB tip collapse
        // previously zeroed Morphine→Normorphine).
        use crate::scene::Primitive;
        let scene = render_fixture(MORPHINE);
        let mut n = 0usize;
        for p in &scene.overlays {
            let Primitive::Path {
                d,
                class: Some(c),
                ..
            } = p
            else {
                continue;
            };
            if !c.split_whitespace().any(|t| t == "edge") || c.contains("head") {
                continue;
            }
            n += 1;
            let nums: Vec<f64> = {
                let mut out = Vec::new();
                let mut cur = String::new();
                for ch in d.chars() {
                    if ch.is_ascii_digit()
                        || ch == '.'
                        || ch == '-'
                        || ch == '+'
                        || ch == 'e'
                        || ch == 'E'
                    {
                        cur.push(ch);
                    } else if !cur.is_empty() {
                        if let Ok(v) = cur.parse::<f64>() {
                            out.push(v);
                        }
                        cur.clear();
                    }
                }
                if !cur.is_empty() {
                    if let Ok(v) = cur.parse::<f64>() {
                        out.push(v);
                    }
                }
                out
            };
            let mut pts = Vec::new();
            let mut i = 0;
            while i + 1 < nums.len() {
                pts.push((nums[i], nums[i + 1]));
                i += 2;
            }
            let mut len = 0.0;
            for w in pts.windows(2) {
                len += (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
            }
            assert!(
                len >= 12.0,
                "shaft too short ({len:.1}px) for {c}: {}",
                &d[..d.len().min(80)]
            );
        }
        assert!(n >= 15, "expected ~20 edges, got {n}");
    }

    #[test]
    fn morphine_sulfate_no_extreme_detours() {
        // Intermediate dashed edges used to take page-edge U-loops (~2.5×
        // chord). Prefer local bows; allow some weave but not frame highways.
        use crate::scene::Primitive;
        let scene = render_fixture(MORPHINE);
        let mut worst = 1.0_f64;
        for p in &scene.overlays {
            let Primitive::Path {
                d,
                class: Some(c),
                ..
            } = p
            else {
                continue;
            };
            if !c.split_whitespace().any(|t| t == "edge") || c.contains("head") {
                continue;
            }
            let nums: Vec<f64> = {
                let mut out = Vec::new();
                let mut cur = String::new();
                for ch in d.chars() {
                    if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' || ch == 'e' || ch == 'E'
                    {
                        cur.push(ch);
                    } else if !cur.is_empty() {
                        if let Ok(v) = cur.parse::<f64>() {
                            out.push(v);
                        }
                        cur.clear();
                    }
                }
                if !cur.is_empty() {
                    if let Ok(v) = cur.parse::<f64>() {
                        out.push(v);
                    }
                }
                out
            };
            let mut pts = Vec::new();
            let mut i = 0;
            while i + 1 < nums.len() {
                pts.push((nums[i], nums[i + 1]));
                i += 2;
            }
            if pts.len() < 2 {
                continue;
            }
            let chord = (pts[0].0 - pts[pts.len() - 1].0).hypot(pts[0].1 - pts[pts.len() - 1].1);
            if chord < 40.0 {
                continue;
            }
            let mut len = 0.0;
            for w in pts.windows(2) {
                len += (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
            }
            let ratio = len / chord;
            worst = worst.max(ratio);
            assert!(
                ratio < 2.15,
                "edge detour too long: path={len:.0} chord={chord:.0} ratio={ratio:.2} d={}",
                &d[..d.len().min(100)]
            );
        }
        let _ = worst;
    }
}
