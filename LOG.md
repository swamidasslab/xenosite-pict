# Lab log

## 2026-10-10

- Coverage pass (Python `coverage` + Rust `llvm-cov`): deleted 0% modules
  `model`/`render`/`scene`/`schema`/`structure`/`perception`, empty
  `backends/`, draft `coordgen/ffi.*`, obsolete
  `rdkit_vs_chematic_layout.ipynb`, RDKit-only edge/wedge test files; slimmed
  `client`/`api` (no dead layout helpers); fixed `make sync` (dropped
  `--extra rdkit`) and `rust_coverage.sh` llvm-tools path.
- Cut rigid/Kabsch fallback and Python RDKit host surface: deleted
  `xpict-core` `align.rs` + PyO3 `kabsch_2d`/`rigid_align_coords`; slimmed
  `align.py` to CoordGen-only (no `Aligner`/`RigidAligner`); stubbed
  `client.render` / `process_edge_plan` (no free-layout fallback); dropped
  those from the public `__init__` exports; removed `test_align.py` /
  `test_rdkit_backend.py`.
- Removed all Indigo references: deleted `backends/indigo.py` and
  `js/.../indigo-wasm.ts`, dropped `xpict[indigo]` / `epam-indigo` from
  `pyproject.toml`, emptied the Python layout-backend registry, and rewrote
  docs/POC captions that named Indigo. Legacy Pict tests that needed a
  chem-engine backend now `pytest.xfail` pending a native CoordGen Pict path.
- Cost align (no Python MCS): Rust atom–bond incidence graph mapper in
  `align_cost.rs` (unmatched atom/bond cost 5 > pair mismatches; element
  mismatch allowed; E/Z substituent sides hard-disallowed; tetrahedral free).
  Constrained CoordGen align in `align_layout.rs` + PyO3
  `cost_mapping` / `align_molecule_constrained`. `align.py` is a thin
  CoordGen-only wrapper (no rigid fallback).
  Key case: hexane→cyclohexane full map, soft-constrained ring stays hexagonal.
- Promoted CoordGen from `poc/` + draft `coordgen/ffi.*` into workspace
  `crates/coordgen-sys` (fixed C ABI against real `sketcherMinimizer`).
  Feature `coordgen` on xpict-core; default-on for xpict-py + xpict; optional
  off for xpict-wasm (emscripten/JS checks postponed). Vendor at
  `vendor/coordgenlibs` (v3.0.2).
- Audited draft FFI vs upstream: dropped fictional
  `template_strength` / `overlap_penalty` / `extra_box` / `collision_score` /
  `setForcePlanar`. Mapped real knobs — `setEvenAngles`,
  `setForceOpenMacrocycles`, precision ctor, metal ZOBs, skip minimization;
  bond stereo via `setStereoChemistry` + stereo-atom neighbors.
- Validation before any RDKit cut: expanded `tests/test_coordgen_native.py`
  (flags, charged, stereo, phenol→anisole constrained); Rust multi-atom
  constrained unit test; notebook
  `notebooks/coordgen_aligned_depiction.ipynb` (native free/constrained vs
  RDKit `GenerateDepictionMatching2DStructure`). `cargo test -p coordgen-sys`
  + pytest green; notebook cells smoke-run. Scale note: CoordGen ~50 vs
  paint `SCALE=20`.
- Interactive playground notebook
  `notebooks/coordgen_playground.ipynb`: edit ref/query SMILES, MATCH pairs,
  constraint mode, and layout FLAGS; view numbered maps + native depictions.
- Alignment notebooks were misleading: independent `a1aaaaa1` maps on fused
  pairs (naphthalene↔quinoline) locked the wrong ring; side-by-side auto-fit
  SVGs also hide true overlap. Fixed mapper to MCS (`CompareAny`); shared
  helpers in `notebooks/_coordgen_align.py` + `tests/test_coordgen_align_notebook.py`
  (hard RMS≈0, fused map ≥9 atoms).
- RDKit baseline overlay: `preferCoordGen` left ~0.005 matched-atom residual
  (looked unaligned). Now `ConstrainedDepictionParams.forceRDKit=True` and seed
  ref from native CoordGen coords so baseline shares the template frame; RMS=0.
- Stereo cases in notebooks/tests: E/Z from RDKit `GetStereoAtoms` →
  `stereo`/`stereo_atom1`/`stereo_atom2`; geometry asserts same-side for cis.
  Chiral SMILES layout smoke (no wedge API in CoordGen FFI). `even_angles` =
  equal neighbor spacing vs default organic 120°/tetrahedral preferences.

## 2026-10-07

- CoordGen POC wheel size (release, macOS arm64 abi3; not wired into xpict):
  PyO3 baseline **147 KiB** wheel / **298 KiB** `.so` → with static CoordGen
  **302 KiB** wheel / **629 KiB** `.so` (**+155 KiB** / **+330 KiB**).
- Tree-shake path: Cargo `opt-level=z` + `lto` + `strip`; C++
  `-ffunction-sections -fdata-sections` + `-Wl,-dead_strip`. Static
  `libcoordgen_sys.a` is **1.5 MiB**; only ~22% of that lands in the linked
  `.so` text growth (~267 KiB `__text` delta).

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
