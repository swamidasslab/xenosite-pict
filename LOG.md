# Lab log

## 2026-09-23

- Centralized document two-pass in ``xpict-core``: ``plan_edge`` → host
  ``process_edge_plan`` → ``render_doc``. CX / star / shade / color chrome
  applied in core ``render_doc`` (not client CX helpers). Native Rust + JS
  ``depict`` use this path; py/wasm expose ``plan_edge`` / ``render_doc`` only.
- Moved CXSMILES parse + ``apply_cx_by_index`` into ``xpict-core::cxsmiles``;
  Rust single-mol ``apply_opts`` calls core (no new client FFI for CX).

## 2026-09-22

- User-facing ``weight`` is relative to house: default ``1`` → absolute ink ``WEIGHT_AT_ONE`` (1.5); min ``WEIGHT_MIN`` (2/3) → Regular stem. Single conversion in ``diagram_weight``; helpers always take the public weight. JS/Python client + schemas/docs drop ``bold_labels`` for ``weight``/``scale``.
- Halo knockout grows with √weight (`halo_gap_for_weight` / `halo_stroke_for_weight`), not linearly with ink stroke.
- Weight also lengthens bond↔label standoff: `label_weight_standoff_px = grow + ½×extra stroke`, threaded through `place_backbone` / `place_label_weighted` so heavier ink does not collide with thickened stems.
- Added mol `weight` (float `>= 1`, default `1.0`): backbone stroke × weight; label glyphs thickened with outward `Shape::buffer` (`grow = 0.5 × stem_px × (weight - 1)`). Removed obsolete `bold_labels`.
- Wired Rust `MoleculeIn` / `depict_molecule`, crates/xpict options, Python `render` paint path; schemas regenerated.
- Added mol `scale` (float, default `1.0`) on live DepictSpec / MoleculeSpec / Rust `MoleculeIn`.
- Uniform diagram scale: paint at house metrics, then scale scene/viewport (font outlines, stroke, pad, geometry). `1.0` matches prior output.
- Wired Python `render` path + Rust `depict_molecule` / crates/xpict `MolRenderOptions`. Schema regenerated.
