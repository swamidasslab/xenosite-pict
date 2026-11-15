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
    /// Review `scheme/fan` — hub plus seven products (network path).
    const FAN: &str = include_str!("../../tests/fixtures/scheme/fan.json");

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
    fn network_fan_all_spokes_are_straight_tip_chords() {
        // Review fan: every hub→product shaft should be a straight tip chord —
        // no hub wraps, no intersections, no ink overlap. Ink-clean tips must
        // not be rejected for padded tile whitespace alone.
        use crate::scene::Primitive;
        let scene = render_fixture(FAN);
        shaft_interior_clear_of_mols(&scene, -1.5);
        let mut shafts = Vec::new();
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
                        if let Ok(v) = cur.parse() {
                            out.push(v);
                        }
                        cur.clear();
                    }
                }
                if !cur.is_empty() {
                    if let Ok(v) = cur.parse() {
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
            let tip = (pts[0].0 - pts[pts.len() - 1].0).hypot(pts[0].1 - pts[pts.len() - 1].1);
            let mut len = 0.0;
            for w in pts.windows(2) {
                len += (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
            }
            let ratio = len / tip.max(1.0);
            assert!(
                ratio < 1.08,
                "fan spoke must be a near tip chord: ratio={ratio:.2} d={}",
                &d[..d.len().min(120)]
            );
            shafts.push(pts);
        }
        assert_eq!(shafts.len(), 7, "expected 7 fan shafts, got {}", shafts.len());
        // Tip chords from one hub must not properly cross each other.
        fn orient(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
            (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
        }
        fn proper_cross(p: (f64, f64), q: (f64, f64), r: (f64, f64), s: (f64, f64)) -> bool {
            let o1 = orient(p, q, r).signum();
            let o2 = orient(p, q, s).signum();
            let o3 = orient(r, s, p).signum();
            let o4 = orient(r, s, q).signum();
            o1 != 0.0 && o2 != 0.0 && o3 != 0.0 && o4 != 0.0 && o1 != o2 && o3 != o4
        }
        for i in 0..shafts.len() {
            for j in (i + 1)..shafts.len() {
                let a0 = shafts[i][0];
                let a1 = shafts[i][shafts[i].len() - 1];
                let b0 = shafts[j][0];
                let b1 = shafts[j][shafts[j].len() - 1];
                assert!(
                    !proper_cross(a0, a1, b0, b1),
                    "fan spokes must not cross: {i} vs {j}"
                );
            }
        }
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
    fn morphine_yellow_head_aims_at_final_target_centroid() {
        // Yellow Morphine→xbm:0007817: target may be soft-nudged off the ELK
        // chord; the head must still aim at that mol's *final* placement
        // centroid (viewport chem center), not a pre-nudge lateral.
        use crate::catmull::open_catmull_end_frame;
        use crate::scene::Primitive;
        let scene = render_fixture(MORPHINE);
        let mut yellow: Option<Vec<(f64, f64)>> = None;
        for p in &scene.overlays {
            let Primitive::Path {
                d,
                class: Some(c),
                stroke: Some(color),
                ..
            } = p
            else {
                continue;
            };
            if color != "#f0e442" || !c.contains("edge") || c.contains("head") {
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
                        if let Ok(v) = cur.parse() {
                            out.push(v);
                        }
                        cur.clear();
                    }
                }
                if !cur.is_empty() {
                    if let Ok(v) = cur.parse() {
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
            yellow = Some(pts);
            break;
        }
        let pts = yellow.expect("yellow #f0e442 Morphine→7817 shaft");
        assert!(pts.len() >= 2, "yellow shaft pts={pts:?}");
        let vp = scene
            .viewports
            .iter()
            .find(|v| v.id.as_deref() == Some("xbm:0007817"))
            .expect("7817 viewport");
        // Final placement centroid proxy: chem sits in the nudged viewport box.
        let cb = (vp.x + vp.width * 0.5, vp.y + vp.height * 0.5);
        let (tip, tan) = open_catmull_end_frame(&pts);
        let to_c = (cb.0 - tip.0, cb.1 - tip.1);
        let clen = to_c.0.hypot(to_c.1).max(1e-9);
        let aim = (tan.0 * to_c.0 + tan.1 * to_c.1) / clen;
        let perp = (to_c.0 * tan.1 - to_c.1 * tan.0).abs() / clen;
        assert!(
            aim > 0.5 && perp < 0.35,
            "yellow head must aim at final 7817 centroid: aim={aim:.3} perp={perp:.3} tip={tip:?} tan={tan:?} cb={cb:?} pts={pts:?}"
        );
    }

    #[test]
    fn morphine_sulfate_no_extreme_detours() {
        // Same length budget for dashed and solid — formatting must not change
        // routing. Short fan bows stay; page-edge U-loops do not.
        use crate::scene::Primitive;
        let scene = render_fixture(MORPHINE);
        let max_ratio = 1.35;
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
                ratio < max_ratio,
                "edge detour too long: path={len:.0} chord={chord:.0} ratio={ratio:.2} (max {max_ratio}) d={}",
                &d[..d.len().min(100)]
            );
        }
        let _ = worst;
    }
}
