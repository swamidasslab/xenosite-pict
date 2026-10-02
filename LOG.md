# Lab log

## 2026-10-01

- Default 2D coords: enable Schrödinger **CoordGen** for free layout when available (Python `prefer_coordgen`, JS `set_new_coords(true)`, native `RDDepict::preferCoordGen`). Constrained align tries CoordGen then `forceRDKit` if the core won’t lock; MinimalLib stays `useCoordGen:false`. Rigid phenol/anisole tolerance loosened to 1e-3 (CoordGen ~5e-5 RMSD).
- Downloaded RDKit/CoordGen ring templates → `notebooks/data/coordgen_ring_templates.smi` (~582 CXSMILES); gallery notebook `notebooks/coordgen_templates_gallery.ipynb`.
- Notebook `notebooks/rdkit_vs_chematic_layout.ipynb`: side-by-side RDKit
  CoordGen vs Chematic native SVG for the same SMILES (no xpict). Procrustes
  overlay allows reflection. Needs `chematic` + `matplotlib` in the venv.
- **Verdict:** Chematic layout is not a drop-in for RDKit depiction. Simple
  fused aromatics match (~0 RMSD). Failures cluster on polycyclic/bridged/
  stereo: morphine (11 bond crossings, ~3.8× bond stretch), adamantane /
  norbornane / barrelene / cubane (crossings), penicillin/caffeine high
  RMSD. No relaxation knobs — rule-based placer only. Stay on RDKit for
  xpict; Chematic not a WASM size win worth the layout quality hit.

## 2026-09-23 (codegen)

- Spike rejected **typebridge** (wrong serde tag rename / broken untagged
  AlignTo in Python). Pivoted to **schemars + ts-rs** for live EdgePlan
  schema + JS types (``make types``). Removed defunct ``LayoutResult`` /
  ``schema/layout.schema.json``. See ``docs/dev/typebridge.md``.

## 2026-09-23 (later)

- Plan: Rust-first **live** contracts via typebridge → generated TS + Pydantic;
  **future** PictSpec stays Pydantic SoT. See ``docs/dev/typebridge.md``.

## 2026-09-23

- Centralized document two-pass in ``xpict-core``: ``plan_edge`` → host
  ``process_edge_plan`` → ``render_doc``. CX / star / shade / color chrome
  applied in core ``render_doc`` (not client CX helpers). Native Rust + JS
  ``depict`` use this path; py/wasm expose plan/validate/render_doc.
- Document ``align_to`` is id string or ``{ref, atom_map?, min_atoms?}``
  (no top-level mol ``atom_map``).
- EdgePlan ``validate`` + CX parse live in core; host suites only smoke RDKit
  ``process_edge_plan`` (no triplicated plan/CX/validate tests).
- Moved CXSMILES parse + ``apply_cx_by_index`` into ``xpict-core::cxsmiles``;
  Rust single-mol ``apply_opts`` calls core (no client CX FFI).

## 2026-09-22

- User-facing ``weight`` is relative to house: default ``1`` → absolute ink ``WEIGHT_AT_ONE`` (1.5); min ``WEIGHT_MIN`` (2/3) → Regular stem. Single conversion in ``diagram_weight``; helpers always take the public weight. JS/Python client + schemas/docs drop ``bold_labels`` for ``weight``/``scale``.
- Halo knockout grows with √weight (`halo_gap_for_weight` / `halo_stroke_for_weight`), not linearly with ink stroke.
- Weight also lengthens bond↔label standoff: `label_weight_standoff_px = grow + ½×extra stroke`, threaded through `place_backbone` / `place_label_weighted` so heavier ink does not collide with thickened stems.
- Added mol `weight` (float `>= 1`, default `1.0`): backbone stroke × weight; label glyphs thickened with outward `Shape::buffer` (`grow = 0.5 × stem_px × (weight - 1)`). Removed obsolete `bold_labels`.
- Wired Rust `MoleculeIn` / `depict_molecule`, crates/xpict options, Python `render` paint path; schemas regenerated.
- Added mol `scale` (float, default `1.0`) on live DepictSpec / MoleculeSpec / Rust `MoleculeIn`.
- Uniform diagram scale: paint at house metrics, then scale scene/viewport (font outlines, stroke, pad, geometry). `1.0` matches prior output.
- Wired Python `render` path + Rust `depict_molecule` / crates/xpict `MolRenderOptions`. Schema regenerated.
