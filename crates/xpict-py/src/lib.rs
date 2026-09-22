//! PyO3 extension module `xpict._native`.
//!
//! Keep algorithm logic in `xpict-core`; this file is the Python surface.

#![forbid(unsafe_code)]

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyModule;
use xpict_core::bonds;
use xpict_core::geom::{self, Shape as CoreShape};
use xpict_core::metrics;
use xpict_core::plotdot::PlotDot;

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

    fn simplify(&self, tolerance: f64) -> Self {
        Self::wrap(self.inner.simplify(tolerance))
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
    m.add_function(wrap_pyfunction!(plotdot_rings, m)?)?;
    m.add_function(wrap_pyfunction!(plotdot_disks, m)?)?;
    m.add_function(wrap_pyfunction!(capsule_halo_path_d, m)?)?;
    m.add_function(wrap_pyfunction!(disk_halo_path_d, m)?)?;
    m.add_function(wrap_pyfunction!(elk_layout_json, m)?)?;
    m.add_function(wrap_pyfunction!(face_metrics, m)?)?;
    m.add_function(wrap_pyfunction!(glyph_metrics, m)?)?;
    m.add_function(wrap_pyfunction!(outline_run_em, m)?)?;
    m.add_function(wrap_pyfunction!(compile_text_shapes, m)?)?;
    m.add_class::<PyShape>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("BOND_PX", metrics::BOND_PX)?;
    m.add("OFFSET_PX", metrics::OFFSET_PX)?;
    m.add("STROKE_PX", metrics::STROKE_PX)?;
    m.add("SHADE_FRAC", metrics::SHADE_FRAC)?;
    m.add("HAS_ELK", true)?;
    m.add("HAS_GEOM", true)?;
    m.add("HAS_FONT", true)?;
    Ok(())
}
