//! Generates the project logo as SVG — no image editor, no raster assets.
//!
//! The mark is the Petersen graph, the standard poster child of graph
//! symmetry, drawn in its canonical five-fold layout.  The colouring is not
//! decorative: one vertex is individualised, `canonaut` canonicalizes the
//! resulting coloured graph, and the vertices are painted by the orbits it
//! reports.  The logo is therefore literally output of the library it labels —
//! change the algorithm and the picture changes with it.
//!
//! Individualising one vertex of the Petersen graph splits its automorphism
//! group (order 120) into three orbits: the chosen vertex, its three
//! neighbours, and the remaining six vertices.
//!
//! ```sh
//! cargo run --example logo            # writes assets/logo.svg, logo-dark.svg,
//!                                     # logo-mark.svg
//! ```

use canonaut::structs::{CanonautManager, DenseGraph};
use std::f64::consts::PI;
use std::fmt::Write as _;

/// Outer/inner ring radii and vertex radius, in user units.
const OUTER_RADIUS: f64 = 44.0;
const INNER_RADIUS: f64 = 24.5;
const VERTEX_RADIUS: f64 = 5.8;
const VIEWBOX: f64 = 108.0;

/// Palette: ink for structure, three orbit colours (chosen for contrast in
/// both light and dark surroundings, and distinguishable in greyscale).
const ORBIT_COLORS: [&str; 3] = ["#e2574c", "#f5b731", "#2a78d6"];

struct Theme {
    name: &'static str,
    edge: &'static str,
    vertex_stroke: &'static str,
    wordmark: &'static str,
    accent: &'static str,
}

const LIGHT: Theme = Theme {
    name: "logo",
    edge: "#1d2433",
    vertex_stroke: "#ffffff",
    wordmark: "#1d2433",
    accent: "#6b7280",
};

const DARK: Theme = Theme {
    name: "logo-dark",
    edge: "#e8ecf4",
    vertex_stroke: "#11161f",
    wordmark: "#f2f5fa",
    accent: "#9aa4b2",
};

/// The Petersen graph: outer 5-cycle, inner pentagram, and the spokes joining
/// them.  Vertices 0..5 are the outer ring, 5..10 the inner one.
fn petersen_graph() -> DenseGraph {
    let mut graph = DenseGraph::new(10);
    for index in 0..5 {
        graph.add_edge(index, (index + 1) % 5); // outer cycle
        graph.add_edge(index, index + 5); // spoke
        graph.add_edge(5 + index, 5 + (index + 2) % 5); // inner pentagram
    }
    graph
}

/// Vertex positions for the classic drawing: outer ring pointing up, inner
/// ring rotated with it so the spokes are radial.
fn vertex_positions() -> Vec<(f64, f64)> {
    let center = VIEWBOX / 2.0;
    (0..10)
        .map(|vertex| {
            let ring_index = vertex % 5;
            let radius = if vertex < 5 { OUTER_RADIUS } else { INNER_RADIUS };
            let angle = -PI / 2.0 + (ring_index as f64) * 2.0 * PI / 5.0;
            (center + radius * angle.cos(), center + radius * angle.sin())
        })
        .collect()
}

/// Orbit index (0, 1, 2, …) of each vertex after individualising vertex 0,
/// as computed by `canonaut`.
fn orbits_after_individualising_one_vertex(graph: &DenseGraph) -> Vec<usize> {
    let mut colored = graph.clone();
    let mut colors = vec![1u32; 10];
    colors[0] = 0; // individualise a single vertex
    colored.set_colors(colors);

    let mut manager = CanonautManager::new(10).with_canonization();
    manager.canonize_graph(&colored);

    // `orbits[v]` is the representative vertex of v's orbit; map the distinct
    // representatives onto 0.. in order of first appearance, so the colour
    // assignment is stable.
    let representatives = manager.orbits();
    let mut order: Vec<u32> = Vec::new();
    representatives
        .iter()
        .take(10)
        .map(|&representative| {
            if let Some(position) = order.iter().position(|&seen| seen == representative) {
                position
            } else {
                order.push(representative);
                order.len() - 1
            }
        })
        .collect()
}

fn edges(graph: &DenseGraph) -> Vec<(usize, usize)> {
    let mut list = Vec::new();
    for first in 0..10 {
        for second in (first + 1)..10 {
            if graph.has_edge(first, second) {
                list.push((first, second));
            }
        }
    }
    list
}

fn render_mark(theme: &Theme, positions: &[(f64, f64)], edges: &[(usize, usize)], orbits: &[usize], indent: &str) -> String {
    let mut svg = String::new();

    let _ = writeln!(svg, "{indent}<g stroke-linecap=\"round\">");
    for &(first, second) in edges {
        let (x1, y1) = positions[first];
        let (x2, y2) = positions[second];
        let _ = writeln!(
            svg,
            "{indent}  <line x1=\"{x1:.2}\" y1=\"{y1:.2}\" x2=\"{x2:.2}\" y2=\"{y2:.2}\" stroke=\"{}\" stroke-width=\"3.0\"/>",
            theme.edge
        );
    }
    let _ = writeln!(svg, "{indent}</g>");

    for (vertex, &(x, y)) in positions.iter().enumerate() {
        let _ = writeln!(
            svg,
            "{indent}<circle cx=\"{x:.2}\" cy=\"{y:.2}\" r=\"{VERTEX_RADIUS}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"2.4\"/>",
            ORBIT_COLORS[orbits[vertex] % ORBIT_COLORS.len()],
            theme.vertex_stroke
        );
    }
    svg
}

fn render_logo(theme: &Theme, positions: &[(f64, f64)], edges: &[(usize, usize)], orbits: &[usize]) -> String {
    let mut svg = String::new();
    let _ = writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 380 108\" width=\"380\" height=\"108\" \
         role=\"img\" aria-label=\"canonaut\">"
    );
    let _ = writeln!(svg, "  <title>canonaut</title>");
    let _ = writeln!(
        svg,
        "  <desc>The Petersen graph, coloured by the vertex orbits canonaut computes after \
         individualising one vertex.</desc>"
    );
    svg.push_str(&render_mark(theme, positions, edges, orbits, "  "));
    let _ = writeln!(
        svg,
        "  <text x=\"124\" y=\"62\" font-family=\"ui-monospace, SFMono-Regular, Menlo, Consolas, monospace\" \
         font-size=\"36\" font-weight=\"600\" fill=\"{}\" letter-spacing=\"-0.5\">canonaut</text>",
        theme.wordmark
    );
    let _ = writeln!(
        svg,
        "  <text x=\"126\" y=\"82\" font-family=\"ui-sans-serif, system-ui, -apple-system, Segoe UI, sans-serif\" \
         font-size=\"12.5\" fill=\"{}\" letter-spacing=\"0.6\">graph canonicalization in Rust</text>",
        theme.accent
    );
    let _ = writeln!(svg, "</svg>");
    svg
}

fn render_square_mark(theme: &Theme, positions: &[(f64, f64)], edges: &[(usize, usize)], orbits: &[usize]) -> String {
    let mut svg = String::new();
    let _ = writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {VIEWBOX} {VIEWBOX}\" width=\"{VIEWBOX}\" \
         height=\"{VIEWBOX}\" role=\"img\" aria-label=\"canonaut\">"
    );
    let _ = writeln!(svg, "  <title>canonaut</title>");
    svg.push_str(&render_mark(theme, positions, edges, orbits, "  "));
    let _ = writeln!(svg, "</svg>");
    svg
}

fn main() -> std::io::Result<()> {
    let graph = petersen_graph();
    let positions = vertex_positions();
    let edge_list = edges(&graph);
    let orbits = orbits_after_individualising_one_vertex(&graph);

    let distinct_orbits = orbits.iter().collect::<std::collections::BTreeSet<_>>().len();
    println!("Petersen graph: {} edges, {distinct_orbits} orbits after individualising one vertex", edge_list.len());

    std::fs::create_dir_all("assets")?;
    for theme in [&LIGHT, &DARK] {
        let path = format!("assets/{}.svg", theme.name);
        std::fs::write(&path, render_logo(theme, &positions, &edge_list, &orbits))?;
        println!("wrote {path}");
    }
    std::fs::write("assets/logo-mark.svg", render_square_mark(&LIGHT, &positions, &edge_list, &orbits))?;
    println!("wrote assets/logo-mark.svg");
    Ok(())
}
