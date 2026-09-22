// quick probe via xpict-core
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::solver::Solver;
use i_overlay::float::overlay::OverlayOptions;
use i_overlay::float::simplify::SimplifyShape;

fn count(shapes: &[Vec<Vec<[f64; 2]>>]) -> usize {
    shapes.iter().flat_map(|s| s.iter()).map(|c| c.len()).sum()
}

fn circle(n: usize, r: f64) -> Vec<[f64; 2]> {
    (0..n)
        .map(|i| {
            let t = std::f64::consts::TAU * i as f64 / n as f64;
            [50.0 + r * t.cos(), 50.0 + r * t.sin()]
        })
        .collect()
}

fn main() {
    // dense circle like glyph curve
    let c: Vec<[f64; 2]> = circle(200, 10.0);
    let shapes0 = vec![vec![c.clone()]];
    println!("raw {}", count(&shapes0));

    let d = shapes0.simplify_shape(FillRule::NonZero);
    println!("default i32 {}", count(&d));

    let d16 = shapes0.simplify_shape_as::<i16>(FillRule::NonZero);
    println!("as i16 {}", count(&d16));

    let d64 = shapes0.simplify_shape_as::<i64>(FillRule::NonZero);
    println!("as i64 {}", count(&d64));

    let mut opt = OverlayOptions::<f64>::default();
    opt.clean_result = true;
    let d_clean = shapes0.simplify_shape_custom(FillRule::NonZero, opt, Solver::AUTO);
    println!("clean_result {}", count(&d_clean));

    opt.clean_result = true;
    let d_clean16 = shapes0.simplify_shape_custom_as::<i16>(FillRule::NonZero, opt, Solver::AUTO);
    println!("clean+i16 {}", count(&d_clean16));

    // with hole
    let outer = circle(120, 10.0);
    let inner = circle(80, 5.0);
    let ring = vec![outer, inner];
    let s = ring.simplify_shape(FillRule::EvenOdd);
    println!("ring evenodd shapes={} holes={} pts={}", s.len(), s.first().map(|x| x.len()).unwrap_or(0), count(&s));
    let s16 = ring.simplify_shape_as::<i16>(FillRule::EvenOdd);
    println!("ring i16 shapes={} holes={} pts={}", s16.len(), s16.first().map(|x| x.len()).unwrap_or(0), count(&s16));
}
