//! PyO3 extension module `xpict._native`.
//!
//! Keep algorithm logic in `xpict-core`; this file is the Python surface.

#![forbid(unsafe_code)]

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyModule;
use xpict_core::align;
use xpict_core::bonds;
use xpict_core::depict;
use xpict_core::geom::{self, Shape as CoreShape};
use xpict_core::metrics;
use xpict_core::plotdot::PlotDot;
use xpict_core::scene::MoleculeIn;

/// Parallel spacing for double/triple strokes (RDKit `multipleBondOffset`).
#[pyfunction]
fn multi_bond_offset(length: f64) -> f64 {
    bonds::multi_bond_offset(length)
}

/// Signed offsets of a centered multiple bond along the left normal.
#[pyfunction]
fn centered_displacements(order: f64, off: f64) -> Vec<f64> {
    bonds::centered_displacements(order, off)
}

/// Paint one molecule: `MoleculeIn` JSON → `Scene` JSON (MVP ABI).
#[pyfunction]
fn depict_molecule(molecule_json: &str) -> PyResult<String> {
    let mol: MoleculeIn = serde_json::from_str(molecule_json)
        .map_err(|e| PyRuntimeError::new_err(format!("MoleculeIn JSON: {e}")))?;
    let scene = depict::depict_molecule(&mol);
    serde_json::to_string(&scene)
        .map_err(|e| PyRuntimeError::new_err(format!("Scene JSON: {e}")))
}

/// Validate `EdgePlan` JSON (unique ids, structure fields, root align=null).
#[pyfunction]
fn validate_edge_plan(plan_json: &str) -> PyResult<String> {
    let plan: xpict_core::EdgePlan = serde_json::from_str(plan_json)
        .map_err(|e| PyRuntimeError::new_err(format!("EdgePlan JSON: {e}")))?;
    plan.validate().map_err(PyRuntimeError::new_err)?;
    serde_json::to_string(&plan)
        .map_err(|e| PyRuntimeError::new_err(format!("EdgePlan JSON: {e}")))
}

/// Pass 1: `DepictSpec` JSON → `EdgePlan` JSON (or ``null`` when empty).
#[pyfunction]
fn plan_edge(spec_json: &str) -> PyResult<String> {
    let spec: xpict_core::DepictSpec = serde_json::from_str(spec_json)
        .map_err(|e| PyRuntimeError::new_err(format!("DepictSpec JSON: {e}")))?;
    let plan = xpict_core::plan_edge(&spec).map_err(PyRuntimeError::new_err)?;
    serde_json::to_string(&plan)
        .map_err(|e| PyRuntimeError::new_err(format!("EdgePlan JSON: {e}")))
}

/// Pass 2: `DepictSpec` + `EdgeResult` JSON → list of `{id, molecule, scene}`.
#[pyfunction]
fn render_doc(spec_json: &str, edge_json: &str) -> PyResult<String> {
    let spec: xpict_core::DepictSpec = serde_json::from_str(spec_json)
        .map_err(|e| PyRuntimeError::new_err(format!("DepictSpec JSON: {e}")))?;
    let edge: xpict_core::EdgeResult = serde_json::from_str(edge_json)
        .map_err(|e| PyRuntimeError::new_err(format!("EdgeResult JSON: {e}")))?;
    let painted = xpict_core::render_doc(&spec, &edge).map_err(PyRuntimeError::new_err)?;
    let rows: Vec<serde_json::Value> = painted
        .into_iter()
        .map(|p| {
            serde_json::json!({
                "id": p.id,
                "molecule": p.molecule,
                "scene": p.scene,
            })
        })
        .collect();
    serde_json::to_string(&rows)
        .map_err(|e| PyRuntimeError::new_err(format!("DocPaint JSON: {e}")))
}

/// Element symbol for atomic number (`0` → ``*``, `1` → ``H``, …).
#[pyfunction]
fn element_symbol(z: u32) -> &'static str {
    xpict_core::element_symbol(z)
}

/// Atomic number for an element symbol (case-insensitive).
#[pyfunction]
fn atomic_number(symbol: &str) -> Option<u32> {
    xpict_core::atomic_number(symbol)
}

/// Kabsch 2D: paired ``src`` → ``dst``. Returns ``(cos, sin, tx, ty, det)``.
#[pyfunction]
#[pyo3(signature = (src, dst, allow_reflect=true))]
fn kabsch_2d(
    src: Vec<(f64, f64)>,
    dst: Vec<(f64, f64)>,
    allow_reflect: bool,
) -> PyResult<(f64, f64, f64, f64, f64)> {
    if src.len() != dst.len() {
        return Err(PyRuntimeError::new_err("src/dst length mismatch"));
    }
    let xf = align::kabsch_2d(&src, &dst, allow_reflect);
    Ok((xf.cos, xf.sin, xf.tx, xf.ty, xf.det))
}

/// Rigid-align ``other`` coords onto a template.
///
/// ``template`` / ``other`` are ``(index, x, y)``. ``mapping`` is
/// ``(other_index, template_index)``. Returns transformed ``(index, x, y)``.
#[pyfunction]
fn rigid_align_coords(
    template: Vec<(i32, f64, f64)>,
    other: Vec<(i32, f64, f64)>,
    mapping: Vec<(i32, i32)>,
) -> Vec<(i32, f64, f64)> {
    align::rigid_align_coords(&template, &other, &mapping).0
}

/// PlotDot rings for one normalized score `z`.
#[pyfunction]
#[pyo3(signature = (z, levels=4))]
fn plotdot_rings(z: f64, levels: usize) -> Vec<(f64, f64)> {
    PlotDot::new(levels).rings(z)
}

/// PlotDot disks: list of `(radius_frac, color_z, x, y)`.
#[pyfunction]
#[pyo3(signature = (zs, coords, levels=4))]
fn plotdot_disks(zs: Vec<f64>, coords: Vec<(f64, f64)>, levels: usize) -> Vec<(f64, f64, f64, f64)> {
    PlotDot::new(levels)
        .disks(&zs, &coords)
        .into_iter()
        .map(|d| (d.radius_frac, d.color_z, d.x, d.y))
        .collect()
}

/// SVG path `d` for a capsule ink halo (bond shaft knockout).
#[pyfunction]
#[pyo3(signature = (x1, y1, x2, y2, ink_radius, grow))]
fn capsule_halo_path_d(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    ink_radius: f64,
    grow: f64,
) -> Option<String> {
    geom::capsule_halo_path_d(x1, y1, x2, y2, ink_radius, grow)
}

/// SVG path `d` for a disk ink halo.
#[pyfunction]
#[pyo3(signature = (cx, cy, ink_radius, grow))]
fn disk_halo_path_d(cx: f64, cy: f64, ink_radius: f64, grow: f64) -> Option<String> {
    geom::disk_halo_path_d(cx, cy, ink_radius, grow)
}

/// Lay out an ELK JSON graph; returns laid-out JSON (native elkrs).
#[pyfunction]
fn elk_layout_json(graph_json: &str) -> PyResult<String> {
    xpict_core::elk_layout_json(graph_json).map_err(PyRuntimeError::new_err)
}

/// Map scheme layout opts → ELK layoutOptions JSON object.
///
/// ``kind`` is ``"reaction"`` or ``"network"``. ``opts_json`` is a
/// [`LayoutOpts`] object (may be ``{}``).
#[pyfunction]
#[pyo3(signature = (kind, opts_json="{}"))]
fn scheme_elk_options(kind: &str, opts_json: &str) -> PyResult<String> {
    let opts: xpict_core::LayoutOpts = serde_json::from_str(opts_json)
        .map_err(|e| PyRuntimeError::new_err(format!("LayoutOpts JSON: {e}")))?;
    let dk = match kind {
        "reaction" => xpict_core::ElkDiagramKind::Reaction,
        _ => xpict_core::ElkDiagramKind::Network,
    };
    let map = xpict_core::scheme_layout_options(dk, &opts);
    serde_json::to_string(&map)
        .map_err(|e| PyRuntimeError::new_err(format!("options JSON: {e}")))
}

/// Viewport-boundary anchors: ``src``/``tgt`` are ``[x,y,w,h]``.
#[pyfunction]
#[pyo3(signature = (src, tgt, pad=None))]
fn edge_anchors(
    src: (f64, f64, f64, f64),
    tgt: (f64, f64, f64, f64),
    pad: Option<f64>,
) -> ((f64, f64), (f64, f64)) {
    xpict_core::edge_anchors(src, tgt, pad.unwrap_or(xpict_core::ANCHOR_GAP))
}

/// Simplify a route polyline JSON ``[[x,y],…]`` → same shape (kink snap).
#[pyfunction]
#[pyo3(signature = (pts_json, kink_px=None))]
fn simplify_route(pts_json: &str, kink_px: Option<f64>) -> PyResult<String> {
    let pts: Vec<(f64, f64)> = serde_json::from_str(pts_json)
        .map_err(|e| PyRuntimeError::new_err(format!("pts JSON: {e}")))?;
    let out = xpict_core::simplify_route(&pts, kink_px.unwrap_or(xpict_core::KINK_PX));
    serde_json::to_string(&out).map_err(|e| PyRuntimeError::new_err(format!("{e}")))
}

/// SVG path ``d`` for a shaft. ``routing`` is ``orthogonal`` / ``polyline`` / ``splines`` / None.
#[pyfunction]
#[pyo3(signature = (pts_json, routing=None))]
fn shaft_path_d(pts_json: &str, routing: Option<&str>) -> PyResult<String> {
    let pts: Vec<(f64, f64)> = serde_json::from_str(pts_json)
        .map_err(|e| PyRuntimeError::new_err(format!("pts JSON: {e}")))?;
    let r = match routing {
        Some("orthogonal") => Some(xpict_core::EdgeRouting::Orthogonal),
        Some("polyline") => Some(xpict_core::EdgeRouting::Polyline),
        Some("splines") => Some(xpict_core::EdgeRouting::Splines),
        _ => None,
    };
    Ok(xpict_core::shaft_path_d(&pts, r))
}

/// Filleted polyline path ``d`` (quadratic corners). ``radius`` defaults to ``TURN_RADIUS``.
#[pyfunction]
#[pyo3(signature = (pts_json, radius=None))]
fn filleted_path_d(pts_json: &str, radius: Option<f64>) -> PyResult<String> {
    let pts: Vec<(f64, f64)> = serde_json::from_str(pts_json)
        .map_err(|e| PyRuntimeError::new_err(format!("pts JSON: {e}")))?;
    Ok(xpict_core::filleted_path_d(
        &pts,
        radius.unwrap_or(xpict_core::TURN_RADIUS),
    ))
}

/// Build edge overlay primitives JSON from an edge paint request.
///
/// Request keys: ``pts``, ``arrow``, ``routing?``, ``color?``, ``stroke_width?``,
/// ``dashed?``, ``label?``, ``label_pos?``, ``index?``.
#[pyfunction]
fn edge_overlay_primitives(edge_json: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct In {
        pts: Vec<(f64, f64)>,
        #[serde(default)]
        arrow: xpict_core::EdgeArrow,
        #[serde(default)]
        routing: Option<xpict_core::EdgeRouting>,
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        stroke_width: Option<f64>,
        #[serde(default)]
        dashed: bool,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        label_pos: Option<String>,
        #[serde(default)]
        index: usize,
    }
    let raw: In = serde_json::from_str(edge_json)
        .map_err(|e| PyRuntimeError::new_err(format!("edge JSON: {e}")))?;
    let edge = xpict_core::EdgePaintIn {
        pts: raw.pts,
        arrow: raw.arrow,
        routing: raw.routing,
        color: raw.color,
        stroke_width: raw.stroke_width,
        dashed: raw.dashed,
        label: raw.label,
        label_pos: raw.label_pos,
        index: raw.index,
    };
    let prims = xpict_core::arrow_edge_primitives(&edge);
    // Serialize as a list of tagged dicts for Python PathPrim/TextPrim rebuild.
    let mut out = Vec::with_capacity(prims.len());
    for p in prims {
        match p {
            xpict_core::EdgePrim::Path {
                d,
                stroke,
                fill,
                stroke_width,
                stroke_dasharray,
                class,
            } => out.push(serde_json::json!({
                "kind": "path",
                "d": d,
                "stroke": stroke,
                "fill": fill,
                "stroke_width": stroke_width,
                "stroke_dasharray": stroke_dasharray,
                "cls": class,
            })),
            xpict_core::EdgePrim::Text {
                x,
                y,
                text,
                fill,
                font_size,
                anchor,
                class,
            } => out.push(serde_json::json!({
                "kind": "text",
                "x": x,
                "y": y,
                "text": text,
                "fill": fill,
                "font_size": font_size,
                "anchor": anchor,
                "cls": class,
            })),
        }
    }
    serde_json::to_string(&out).map_err(|e| PyRuntimeError::new_err(format!("{e}")))
}

fn parse_face_style(style: &str) -> xpict_core::font::FaceStyle {
    match style {
        "bold" => xpict_core::font::FaceStyle::Bold,
        "italic" => xpict_core::font::FaceStyle::Italic,
        "bold_italic" => xpict_core::font::FaceStyle::BoldItalic,
        _ => xpict_core::font::FaceStyle::Regular,
    }
}

/// `(upem, ascent, descent, line_gap, cap_height, x_height, stem_em)`.
#[pyfunction]
#[pyo3(signature = (style="regular"))]
fn face_metrics(style: &str) -> (f64, f64, f64, f64, f64, f64, f64) {
    let m = xpict_core::font::face_metrics(parse_face_style(style));
    (
        m.upem,
        m.ascent,
        m.descent,
        m.line_gap,
        m.cap_height,
        m.x_height,
        m.stem_em,
    )
}

/// `(advance, ink_xmin, ink_ymin, ink_xmax, ink_ymax)` — ink may be None via NaN sentinel…  
/// Returns `(advance, Option<(xmin,ymin,xmax,ymax)>)`.
#[pyfunction]
#[pyo3(signature = (ch, style="regular"))]
fn glyph_metrics(ch: &str, style: &str) -> (f64, Option<(f64, f64, f64, f64)>) {
    let c = ch.chars().next().unwrap_or('\0');
    match xpict_core::font::glyph_metrics(c, parse_face_style(style)) {
        Some(g) if g.has_ink() => (
            g.advance,
            Some((
                g.ink_xmin.unwrap(),
                g.ink_ymin.unwrap(),
                g.ink_xmax.unwrap(),
                g.ink_ymax.unwrap(),
            )),
        ),
        Some(g) => (g.advance, None),
        None => (0.0, None),
    }
}

/// Outline plain text in font space; returns `(Shape|None, advance_em)`.
#[pyfunction]
#[pyo3(signature = (text, style="regular"))]
fn outline_run_em(text: &str, style: &str) -> (Option<PyShape>, f64) {
    let (shape, adv) = xpict_core::font::outline_run_em(text, parse_face_style(style));
    (shape.map(PyShape::wrap), adv)
}

/// Compile plain text to SVG-space Shape.
#[pyfunction]
#[pyo3(signature = (text, x, y, font_size, anchor="middle", style="regular"))]
fn compile_text_shapes(
    text: &str,
    x: f64,
    y: f64,
    font_size: f64,
    anchor: &str,
    style: &str,
) -> Option<PyShape> {
    xpict_core::font::compile_text_shapes(
        text,
        x,
        y,
        font_size,
        anchor,
        parse_face_style(style),
    )
    .map(PyShape::wrap)
}

/// Returns `(bond_ends, labels)` where each bond end is `(x1,y1,x2,y2)` and
/// each label is `None` or
/// `(text, origin_x, y, atom_x, atom_y, side, clearance, path_d, raw)`
/// with `side` in `{"east","west","north","south"}`.
#[pyfunction]
#[pyo3(signature = (atoms, bonds, font_size=None, weight=None))]
fn place_backbone(
    atoms: Vec<(f64, f64, Option<String>)>,
    bonds: Vec<(usize, usize)>,
    font_size: Option<f64>,
    weight: Option<f64>,
) -> (
    Vec<(f64, f64, f64, f64)>,
    Vec<Option<(String, f64, f64, f64, f64, String, f64, String, String)>>,
) {
    let font_px = font_size.unwrap_or(metrics::FONT_PX);
    let weight = weight.unwrap_or(1.0);
    let atoms: Vec<xpict_core::labels::AtomIn> = atoms
        .into_iter()
        .map(|(x, y, label)| xpict_core::labels::AtomIn { x, y, label })
        .collect();
    let bonds: Vec<xpict_core::labels::BondIn> = bonds
        .into_iter()
        .map(|(begin, end)| xpict_core::labels::BondIn { begin, end })
        .collect();
    let (outs, labs) = xpict_core::labels::place_backbone(
        &atoms,
        &bonds,
        font_px,
        xpict_core::font::FaceStyle::Regular,
        weight,
    );
    let bond_ends = outs
        .into_iter()
        .map(|b| (b.x1, b.y1, b.x2, b.y2))
        .collect();
    let labels = labs
        .into_iter()
        .map(|opt| {
            opt.map(|p| {
                let side = match p.side {
                    xpict_core::labels::LabelSide::East => "east".to_string(),
                    xpict_core::labels::LabelSide::West => "west".to_string(),
                    xpict_core::labels::LabelSide::North => "north".to_string(),
                    xpict_core::labels::LabelSide::South => "south".to_string(),
                };
                (
                    p.text,
                    p.origin_x,
                    p.y,
                    p.atom_x,
                    p.atom_y,
                    side,
                    p.clearance,
                    p.path_d,
                    p.raw,
                )
            })
        })
        .collect();
    (bond_ends, labels)
}

/// Split label into `(center, traveling)` parts.
#[pyfunction]
fn split_atom_label(raw: &str) -> (String, String) {
    let p = xpict_core::labels::split_label(raw);
    (p.center, p.traveling)
}

/// Compose display string for a side (`"east"` / `"west"`).
#[pyfunction]
fn compose_atom_label(center: &str, traveling: &str, side: &str) -> String {
    let parts = xpict_core::labels::LabelParts {
        center: center.to_string(),
        traveling: traveling.to_string(),
        charge: 0,
    };
    let side = match side.to_ascii_lowercase().as_str() {
        "west" => xpict_core::labels::LabelSide::West,
        "north" => xpict_core::labels::LabelSide::North,
        "south" => xpict_core::labels::LabelSide::South,
        _ => xpict_core::labels::LabelSide::East,
    };
    xpict_core::labels::compose_label(&parts, side)
}

/// Glyph ink shape for a placed chem label (halo occupancy).
#[pyfunction]
#[pyo3(signature = (raw, origin_x, y, atom_x, atom_y, side, font_size=None))]
fn label_ink_shape(
    raw: &str,
    origin_x: f64,
    y: f64,
    atom_x: f64,
    atom_y: f64,
    side: &str,
    font_size: Option<f64>,
) -> Option<PyShape> {
    let font_px = font_size.unwrap_or(metrics::FONT_PX);
    let side = match side.to_ascii_lowercase().as_str() {
        "west" => xpict_core::labels::LabelSide::West,
        "north" => xpict_core::labels::LabelSide::North,
        "south" => xpict_core::labels::LabelSide::South,
        _ => xpict_core::labels::LabelSide::East,
    };
    let pl = xpict_core::labels::PlacedLabel {
        raw: raw.to_string(),
        text: String::new(),
        origin_x,
        y,
        atom_x,
        atom_y,
        side,
        clearance: 0.0,
        path_d: String::new(),
    };
    xpict_core::labels::label_ink_shape(&pl, font_px, xpict_core::font::FaceStyle::Regular)
        .map(PyShape::wrap)
}

/// Multipolygon geometry (Shapely stand-in).
#[pyclass(name = "Shape")]
#[derive(Clone)]
struct PyShape {
    inner: CoreShape,
}

impl PyShape {
    fn wrap(inner: CoreShape) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyShape {
    #[staticmethod]
    fn empty() -> Self {
        Self::wrap(CoreShape::empty())
    }

    #[staticmethod]
    fn from_ring(ring: Vec<(f64, f64)>) -> Self {
        Self::wrap(CoreShape::from_ring(&ring))
    }

    #[staticmethod]
    fn from_contours_evenodd(contours: Vec<Vec<(f64, f64)>>) -> Self {
        Self::wrap(CoreShape::from_contours_evenodd(&contours))
    }

    #[staticmethod]
    #[pyo3(signature = (cx, cy, radius, quad_segs=12))]
    fn disk(cx: f64, cy: f64, radius: f64, quad_segs: u32) -> Self {
        Self::wrap(CoreShape::disk(cx, cy, radius, quad_segs))
    }

    #[staticmethod]
    fn capsule(x1: f64, y1: f64, x2: f64, y2: f64, radius: f64) -> Self {
        Self::wrap(CoreShape::capsule(x1, y1, x2, y2, radius))
    }

    #[staticmethod]
    fn polyline_buffer(pts: Vec<(f64, f64)>, radius: f64) -> Self {
        Self::wrap(CoreShape::polyline_buffer(&pts, radius))
    }

    #[staticmethod]
    fn multipoint_buffer(pts: Vec<(f64, f64)>, radius: f64) -> Self {
        Self::wrap(CoreShape::multipoint_buffer(&pts, radius))
    }

    #[staticmethod]
    fn annular(cx: f64, cy: f64, r: f64, stroke_width: f64) -> Self {
        Self::wrap(CoreShape::annular(cx, cy, r, stroke_width))
    }

    #[getter]
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[getter]
    fn area(&self) -> f64 {
        self.inner.area()
    }

    #[getter]
    fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        self.inner.bounds()
    }

    #[getter]
    fn centroid(&self) -> Option<(f64, f64)> {
        self.inner.centroid()
    }

    #[getter]
    fn has_holes(&self) -> bool {
        self.inner.has_holes()
    }

    fn buffer(&self, dist: f64) -> Self {
        Self::wrap(self.inner.buffer(dist))
    }

    fn halo(&self, dist: f64) -> Self {
        Self::wrap(self.inner.halo(dist))
    }

    fn simplify(&self) -> Self {
        Self::wrap(self.inner.simplify())
    }

    #[getter]
    fn point_count(&self) -> usize {
        self.inner.point_count()
    }

    fn union(&self, other: &Self) -> Self {
        Self::wrap(self.inner.union(&other.inner))
    }

    fn difference(&self, other: &Self) -> Self {
        Self::wrap(self.inner.difference(&other.inner))
    }

    fn xor(&self, other: &Self) -> Self {
        Self::wrap(self.inner.xor(&other.inner))
    }

    fn translate(&self, dx: f64, dy: f64) -> Self {
        Self::wrap(self.inner.translate(dx, dy))
    }

    #[pyo3(signature = (sx, sy, ox=0.0, oy=0.0))]
    fn scale(&self, sx: f64, sy: f64, ox: f64, oy: f64) -> Self {
        Self::wrap(self.inner.scale(sx, sy, ox, oy))
    }

    fn contains(&self, x: f64, y: f64) -> bool {
        self.inner.contains(x, y)
    }

    fn to_svg_d(&self) -> String {
        self.inner.to_svg_d()
    }

    fn horizontal_span_at(&self, y: f64) -> f64 {
        self.inner.horizontal_span_at(y)
    }

    /// List of `(exterior, holes)` rings for introspection / annotate.
    fn polygons(
        &self,
    ) -> Vec<(Vec<(f64, f64)>, Vec<Vec<(f64, f64)>>)> {
        self.inner.polygons_rings()
    }

    fn __bool__(&self) -> bool {
        !self.inner.is_empty()
    }
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(multi_bond_offset, m)?)?;
    m.add_function(wrap_pyfunction!(centered_displacements, m)?)?;
    m.add_function(wrap_pyfunction!(depict_molecule, m)?)?;
    m.add_function(wrap_pyfunction!(validate_edge_plan, m)?)?;
    m.add_function(wrap_pyfunction!(plan_edge, m)?)?;
    m.add_function(wrap_pyfunction!(render_doc, m)?)?;
    m.add_function(wrap_pyfunction!(element_symbol, m)?)?;
    m.add_function(wrap_pyfunction!(atomic_number, m)?)?;
    m.add_function(wrap_pyfunction!(kabsch_2d, m)?)?;
    m.add_function(wrap_pyfunction!(rigid_align_coords, m)?)?;
    m.add_function(wrap_pyfunction!(plotdot_rings, m)?)?;
    m.add_function(wrap_pyfunction!(plotdot_disks, m)?)?;
    m.add_function(wrap_pyfunction!(capsule_halo_path_d, m)?)?;
    m.add_function(wrap_pyfunction!(disk_halo_path_d, m)?)?;
    m.add_function(wrap_pyfunction!(elk_layout_json, m)?)?;
    m.add_function(wrap_pyfunction!(scheme_elk_options, m)?)?;
    m.add_function(wrap_pyfunction!(edge_anchors, m)?)?;
    m.add_function(wrap_pyfunction!(simplify_route, m)?)?;
    m.add_function(wrap_pyfunction!(shaft_path_d, m)?)?;
    m.add_function(wrap_pyfunction!(filleted_path_d, m)?)?;
    m.add_function(wrap_pyfunction!(edge_overlay_primitives, m)?)?;
    m.add_function(wrap_pyfunction!(face_metrics, m)?)?;
    m.add_function(wrap_pyfunction!(glyph_metrics, m)?)?;
    m.add_function(wrap_pyfunction!(outline_run_em, m)?)?;
    m.add_function(wrap_pyfunction!(compile_text_shapes, m)?)?;
    m.add_function(wrap_pyfunction!(place_backbone, m)?)?;
    m.add_function(wrap_pyfunction!(split_atom_label, m)?)?;
    m.add_function(wrap_pyfunction!(compose_atom_label, m)?)?;
    m.add_function(wrap_pyfunction!(label_ink_shape, m)?)?;
    m.add_class::<PyShape>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("BOND_PX", metrics::BOND_PX)?;
    m.add("OFFSET_PX", metrics::OFFSET_PX)?;
    m.add("STROKE_PX", metrics::STROKE_PX)?;
    m.add("SHADE_FRAC", metrics::SHADE_FRAC)?;
    m.add("HAS_ELK", true)?;
    m.add("HAS_GEOM", true)?;
    m.add("KINK_PX", xpict_core::KINK_PX)?;
    m.add("TURN_RADIUS", xpict_core::TURN_RADIUS)?;
    m.add("DEFAULT_NODE_SPACING", xpict_core::LayoutOpts::DEFAULT_NODE_SPACING)?;
    m.add("DEFAULT_LAYER_SPACING", xpict_core::LayoutOpts::DEFAULT_LAYER_SPACING)?;
    m.add("HAS_FONT", true)?;
    Ok(())
}
