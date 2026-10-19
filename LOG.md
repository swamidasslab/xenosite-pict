# Lab log

## 2026-10-19

- Heteroatom H drop: Chematic `edge_resolve::to_molecule_in` labeled bare `O`/`N`. Fix: store `implicit_hcount` on parse and build via shared `atom_display_label` (OH/NH2/NH4). Tests: chematic SMILES labels + `depict` ethanol/aniline paint.
- Charge size/overlap: superscripts shared `SCRIPT_SCALE=0.66` with H-counts and bond insets cleared only the center glyph. Split `SUPERSCRIPT_SCALE=0.90` vs `SUBSCRIPT_SCALE=0.66`; bond clearance uses full label ink AABB so `+`/`−` clear bond ends.

## 2026-10-10

- Mol captions in Rust: `render_doc` folds text-node refs into the mol scene (`mol-caption` glyph paths, default below, font 1.15× atom labels). Host sugar only emits text+label; notebook HTML stays a thin row/cell packer.
- Align MCS bug: AbGraph search scored a bond match when only one endpoint was mapped, so a shifted methyl→ring map undercut the ring+Me↔O map (toluene ← anisole; chain `+` hop-2). Fix: `mapping_cost_ab` only charges bond `pair_cost` when both atom endpoints are mapped (same rule as `mapping_cost`). Ideal pendant pair is Me↔O (exo to ipso), not unmatched methyl.
- Nested scheme compose: `depict` / `iter_cells` run Rust `compose_scheme` for every `reaction_scheme` cell (not only document root). Notebook HTML paints the full doc once, finds rows in the spec tree, SVGs cells — no per-row re-depict / opts injection.
- Host prune: `Mol.to_spec` identity on leaf + cascade only via `.opts` bag (no dual-write). Notebook `display` uses composed scheme cells; dropped Unicode HTML arrows.
- Wire fix: `OptsPatch::Universal` (and `ForTypesPatch`) are [`MolOpts`], not `CommonOpts` — untyped `{weight,halo,shade}` on group/grid bags cascade. Host `.opts(**kwargs)` appends `MolOpts`.
- `PictBase.opts`: shallow-copy root + append cascade patches only. Dropped dual field/`MolOptsPatch` host routing — Rust chrome resolve owns inheritance.
- Host stray cleanup: dropped draw-era `native_bridge` ink/halo/elk wrappers; deleted unused `contracts/layout.py` (`Pict.layout` now returns `list[MoleculeIn]`); removed duplicate `DepictSpec` operator overrides + dead `row_as_spec`; notebook `_repr_*` live on `PictBase` (Rendered keeps its own SVG path).
- Deleted obsolete host modules: `align.py`, `edge_plan.py`, `client.py`, `svg.py`, `data/`. Mol/render live in `spec.py`; Scene→SVG in `rendered.py`; EdgePlan validate via `native_bridge`.
- Deleted obsolete `python/xpict/draw/` (+ `data/fonts`). Ship path uses thin Scene→SVG in `rendered.py` (parity with JS `scene-svg.ts`); paint/glyphs stay in Rust.
- Removed layout-backend swapping: `Pict` / `render(doc)` are native Rust `depict` only (`backend=` gone, `PictBackendWarning` deleted). Legacy flat docs lift to live DepictSpec; `Pict.layout` reads coords from painted rows.
- Deleted obsolete `python/xpict/layout2d.py`; CoordGen tests call `_native.generate_2d_coordinates` directly. Product path remains Rust `depict`.
- Host cleanup finished: `contracts/__init__` is re-exports only (no lazy host DepictSpec); `MolRenderOptions` stays a dataclass (avoids pydantic `Mol`/`Rendered` rebuild); basedpyright-only config (`failOnWarnings=false`, `reportExplicitAny=error`). Reaction-scheme xfails removed — native CoordGen path XPASS→pass.
- Python host slimmed to wrapper + sugar: `contracts/` re-exports generated pydantic/TypedDicts; `sugar.py` holds `+` / `>>` / `|` only; depict path is `native_bridge` → Rust. Deleted hand-rolled opt coerce (pydantic `TypeAdapter(Opts)`); banned `Any` / `object` annotations on the live host (`make check-pyright`).
- Spec → one Rust `depict`: thin `plan_edge` → in-core Chematic+CoordGen `process_edge_plan` → `render_doc` (same shape as the JS wrapper, no JSON shuttle). Python/JS hosts only build DepictSpec. MSRV `1.98`; Chematic `1.0.42`.
- `+` chain-aligns onto the **previous** mol (not hub-to-first). Group `align: true` defaults match.
- Aromatic circle radius factor `AROMATIC_CIRCLE_FRAC = 0.60` (was 0.88).
- Meta test: `tests/test_no_chem_host_imports.py` fails if `python/xpict` imports rdkit / indigo / chematic.
- Cascading opt `debug_align` (default off): colored disks + **template atom index** on matched atoms (same number on template and query); unmatched query atoms get open gray circles; corner badge `align★T` / `align→Q`. Host stamps `AlignDebug` after CoordGen align; Rust paints Overlay. Clear, not pretty.
- Align fix: `COST_ELEMENT` raised above `2 * COST_UNMATCHED` so MCS no longer pads with O→C pairs (anisole onto o-cresol).
- Project rule: one Rust core — zero expected wrapper drift (hosts are types/adapters only). Documented in `AGENTS.md`, `CONTRIBUTING.md`, `.cursor/rules/rust-core-python-adapters.mdc`.
- Cascade is Rust-only DepictSpec semantics: `Grid` variant in core; `resolve_mol_chrome` walks ancestor layers (grid → row → mol). Host calls `resolve_depict_chrome` and paints — deleted `xpict.cascade` / host inherit recursion. `make types` emits `GridNode` with `list[DepictSpecRoot]` children.
- Bugfix: host `render_depict_mols` / HTML dropped grid-root `.opts(…)` when recursing into `|` rows (fixed by moving cascade to Rust).
- Cascading opt `debug_box` (default off): dashed magenta rectangles for label ink AABBs, mol canvas, and full scene (`debug_box.rs` + Overlay / scene overlays). Debug aid only (not publication). Stroke opacity `0.45` so overlaps stay readable. No boxes around atom/bond stroke primitives. Wired through `CommonOpts` / `MolOpts` / host cascade; classes `debug-box-label` / `debug-box-mol` / `debug-box-scene`.
- Aromatic CTAB order `4` / RDKit `1.5` was reaching paint but looked like an all-single hexagon: `depict_order` collapses those to one skeleton stroke and nothing drew the ring circle.
- Decision: keep aromatic bond types; paint a stroked `class="aromatic"` circle for each SSSR ring whose bonds are all aromatic (`aromatic_ring_circles` in `rings.rs`, used by `depict_molecule`).
- Config: cascading opt `aromatic: "circle" | "kekule"` on `CommonOpts` / `MolOpts` / spec roots (inherits to mols). Kekulé invents alternating doubles for paint only. Default `circle`.
- Coverage: Rust `benzene_aromatic_orders_draw_ring_circle` + `benzene_kekule_style_skips_circle_uses_doubles`; e2e circle + kekule opt tests.
- Cascade tests: Rust `cascade_aromatic_global_inherits_local_overrides` + `render_doc_stamps_cascaded_aromatic_onto_molecule`; Python `test_aromatic_opts_global_and_local_resolve_and_print` (group inherit, local override, `_repr_svg_` / HTML).
