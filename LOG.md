# Lab log

## 2026-10-29

- Default scheme `layer_spacing` is 20px (was 0). Flush ELK layers jammed mol captions (`ethanol` / `acetaldehyde`) into one line; a small between-layer gap also lengthens the arrow corridor.
- Dropped `LayoutOpts.refine`. Scheme layout always runs the constraint pass after ELK; there is no stock-ELK opt-out on the wire.
- Coterminal scheme arrows (A→B and B→A, or two A→B) shared a path, so ParallelClear flung one shaft. Group by unordered mol pair / shared terminals, solve one centerline, then miter-offset lanes so they stay a constant distance along the whole path (including curves). Reverse members keep their painted direction. Lane gap is `COTERMINAL_LANE_PX` (8px, compact pair); `SCHEME_EDGE_CLEARANCE` (one bond) still applies between unrelated parallel shafts.

## 2026-10-27

- `@swamidasslab/xpict` is public, but `npm.pkg.github.com` still 401s
  without a token. `js/v0.7.1` GitHub Release now has
  `xenosite-xpict-0.7.1.tgz` (`npm pack` of `@xenosite/xpict`). Publish
  workflows attach that tarball on `js/v*` / `release/v*` so consumers
  (xenobase) do not need `GITHUB_TOKEN`.
- Dropped `grant-package-actions.yml`. `@swamidasslab/xpict` inherits from this
  repo, so `packages/.../actions/permissions` 404s and there is no Manage
  Actions access UI.
- Vendored canonaut 1.0.0 under vendor/canonaut with word size from usize::BITS (same copy as xenobase wasm). crates.io 1.0.0 panics on wasm32 past 32 vertices. Dropped the 32-bit skip in reference_generators; rebuilt js/src/wasm.
- `js/v0.7.1` publish was blocked by Windows native smoke (`stdc++.lib` / coordgen). That matrix cell is `continue-on-error` so JS GitHub Packages can ship.


## 2026-10-22

- Mycophenolate labels were only Outside vs shafts (tiny glyph disks), so they sat on chem. This kit emits `push_clear_leaves` vs painted scheme leaves (chem, other labels) — not a solver-wide all-pairs default. Last interior ELK point sits on the endpoint–centroid ray so the painted last chord (the arrow) aims at the mol. Clamp-to-seed stays on that ray: a south/east port is not a snap, or two-point shafts point at the box corner instead of the centroid.

- Fit leftover (`shift_path_d`, origin-0 viewBox) was a workaround that broke cubics. Crop is now the SVG viewBox (`Scene.x`/`y` + size). Geometry and path `d` stay put; mol `ox`/`oy` still maps paint space onto the ELK box.

- Fit was shifting only SVG `M`/`L` on overlays. Catmull shafts are `C` cubics, so the filled head (an `M L L Z`) moved and the Glutathionation curve did not. `shift_path_d` now translates cubic controls too. The JS loader cache-busts wasm by content hash so a rebuilt blob actually loads.

- Arrow head is the last route control, pulled back along that last chord. Shaft interpolates the same `base`. No SVG `d` scrape for the join (that rounded a cubic trim and left the triangle off the Glutathionation shaft). Bond halo uses `StrokePath.pts` the same way. Tests rebuild the expected triangle from the controls; a 1.5 px path-parse gap no longer hides a miss.

- Tight ELK gaps no longer collapse to a 20 px stub: standoff cannot walk past the ELK seed into the neighbor, and the head scales with stroke (4.8 px → 27 px). Lower-left PEITC isocyanate→GSH arrow was a dart on a fat stroke.

- Arrow head is the last route point. The shaft is trimmed back along the same Catmull cubics; the head base *is* that trim, so the two cannot separate. Shaft control points freeze after the first solve; labels cannot shove the arrow.

- Scheme shafts keep ELK endpoints and routing points as the only path DOF. Catmull–Rom is paint (and clash/tangent sampling) through those vertices, not a densified control polyline. Writing samples back as the path collapsed elbows (`simplify_route`) and looked like extra standoff.

- Edge-label rotation is a solver DOF (`Constraint::Parallel` vs the painted tangent, mod 180°). After minimization, upside-down glyphs flip 180° into (−90, 90]. Soft `Midpoint` springs the label to the live arc-length mid of the shaft; Outside still clears the stroke.

- Scheme arrow standoff is `min(2 bonds, ELK seed distance to the clash grid)`. Two bonds when ELK left a wide box; shrinks to the seed clearance when packing is tight. Not a hard ink wall (that reversed shafts). The old ⅓-bond constant sat on large mols (PEITC GSH adduct).

- Open Catmull–Rom uses phantom end neighbors so the last cubic follows the end chord. Duplicating the endpoint (`p3 = p2`) collapsed the last Bézier and left filled arrow heads off the Glutathionation curve.

- Edge-label clash is the solver, not a paint snap. Outside vs the Catmull through ELK control points; clash is half the edge stroke + pad. The label is a glyph disk, not the unrotated text AABB (that box swallowed the bulge). MaxDist ceiling is clash + one bond so CLEAR can push.

- Product 0.7.1: clash-grid cleanup (opt-in ink raster; Outside stays geometric).

- Clash grid is opt-in (`add_ink` / `ink_signed`). Outside energy stays geometric so captions/callouts still clear the chem AABB, not the raster. Stroke-only scene paths are not filled AABBs (bond Lines already cover them). Unused label-ink constant dropped.

- Layout nodes own painted ink (`add_ink`) and expose an on-demand cached clash grid (chamfer SDF). Scheme arrows standoff from that grid — bonds, heteroatom glyphs, regions, annotations — not a kit-side leaf dump. Halo/debug paint is omitted.

- Scheme arrow ink standoff is BOND_PX/3 (~10 px). One bond left heads sitting in the ELK gap instead of reaching the chem.

- Scheme arrow standoff is from chem ink (one bond), along a ray through the mol centroid. Energy is RayThrough (min |signed distance of the infinite shaft line to the centroid|²). The ray may cross ink; only the painted end stops. Not isotropic Outside vs chem — that shoved shafts off the aiming line.

- Scheme arrow standoff is MinimizeDist at 1.5 bonds from the framed centroid, on the ELK-facing ray (capped at 40% of the centroid–centroid span). Not a hard ink wall: that shoved infeasible ELK ports off the ray and reversed shafts.

- Scheme refine primary energy is MinimizeDist from each shaft endpoint to that mol’s framed centroid (`frame_offset` + scale, same space as painted ink). Hard ink standoff stops them short. Weak Collinear to the frozen ELK neighbor only breaks the standoff-circle tie so the shaft does not swap ends. Labels wrap to (−90, 90].

- Rotated edge labels use `dominant-baseline="central"` so SVG `rotate` is about the glyph center, not the alphabetic baseline (corner).

- Scheme arrow ink standoff is 1.5 bonds. Endpoints initialize on the centroid-aiming ray (not ELK box ports). Energy is MinimizeDist to the centroid plus Collinear so the shaft tangent aims at the center; hard chem clearance keeps them from reaching it.

- Product 0.7.0: ELK-then-energy reaction-scheme refine (`LayoutOpts.refine`; `false` keeps stock ELK). Nested mol SVG groups; leftover fit on viewport `ox`/`oy`.

- Reaction-scheme default is ELK seed then constraint refine (`LayoutOpts.refine`; `false` keeps stock ELK). Mols stay fixed; movable DOF are ELK control points and edge labels. Shafts paint as open centripetal Catmull–Rom from a shared `catmull` module (not duplicated in `arrows`, not via `doc`). Standoff is from chem ink (1.5 bonds), not the mol AABB. ParallelClear is hard; PolyCross only for pairs that did not already cross at the seed. Labels rotate with the shaft and MaxDist-tether to it.

- Network GSH region / hub aromatic bugs were hierarchy, not path-command special cases. One `g.xpict-mol` with nested layer groups (shading, halo, bonds, …). Fit leftover lives on viewport `ox`/`oy` (nested translate). Chrome+captions paint in chem space, then one mol-tree `scale`. Regions and GSH labels share the group transform.

## 2026-10-21

- Dropped leftover xenopict `MoleculeIn.mark_*` / host `MolRenderOptions.mark_*` (never reached CircleSpec). Publication marks are `circle` overlays only. Removed JS SVG stub, unused frame math, elkjs POC, and the `xpict_depict_molecule` C alias.

- Product 0.6.1: `circle.bonds` midpoint circles; ELK scheme ports slide toward the other mol (`PORT_POSITION`).

- ELK scheme default: `portConstraints=FIXED_SIDE` and network-simplex `nodeFlexibility=PORT_POSITION` so dummy ports slide along the flow side toward the other mol (straighter arrows).

- `circle.bonds` is a circle at the bond midpoint, same fill/stroke/scale and bond-clearance as `circle.atoms`. Capsule geometry is unused. Class `bond-circle`.

- Product 0.6.0: one Rust scene_to_svg (`g.xpict-mol[data-id]`, no XML decl); scheme arrows follow the viewport group.

- Scheme arrows were high vs mols: compose overwrote fit's leftover viewport origin. Placement `x`/`y` is the SVG group translate; bake origin into local primitives so ink sits in the box.

- Product 0.5.2: one Rust `scene_to_svg` for all hosts; `g.xpict-mol[data-id]`; no XML declaration.

- Scene→SVG lives in `xpict-core` only. Viewport spec ids become `g.xpict-mol[data-id]` (escaped). No `<?xml …?>` (inline HTML). Python/JS call `_native.scene_to_svg` / `xpict_scene_to_svg`.

- Product 0.5.1: drop RDKit, Emscripten CoordGen wasm, reaction_scheme JS smoke.

- JS wasm is `wasm32-unknown-emscripten` (`em++` compiles CoordGen and links libc++). `wasm32-unknown-unknown` has no `<map>`. `scripts/ensure_emsdk.sh` installs emsdk 6.0.13 under `~/.cache/emsdk`. C ABI `xpict_render` + MODULARIZE ES module; wasm-pack/wasm-bindgen dropped for this crate. Orthogonal scheme arrows: elkrs can emit a sub-4px shaft when layers pack flush; fall back to viewport anchors.

- JS scheme smoke: `xpict.depict({ type: "reaction_scheme" })` packs two (and three) mols, edge overlays, arrow styles, orthogonal routing, spacing knobs, and the same mol annotations as a lone mol.

- Deleted the unused JS RDKit layout path (`rdkit-layout.ts`, `rdkit-loader.ts`) and the CX helper that only existed to feed it. Product JS sends specs to Rust `render`; `frame.ts` stays (types / SVG-frame math, no RDKit).

- JS `depict` / `depictMolecule` stay as single-mol helpers: they wrap a mol as a DepictSpec and call the same Rust `render(spec)`. Not a parallel `MoleculeIn` paint path.

- Dropped the RDKit stack from `crates/xpict` (crates.io `rdkit`, cxx Depictor FFI, GraphMol `compat/` includes, `CPLUS_INCLUDE_PATH`) and unused JS `@rdkit/rdkit`. Native `render` builds a `DepictSpec` and calls core `depict` (Chematic + CoordGen). `align_to` is a template source string; Rust computes coords. CI/docs no longer install `librdkit-dev`.

- Root mol is `MolNode` (`DepictSpec::Mol(MolNode)`), the same type as scheme/group children — one annotation schema (`region`, `circle`, `callout`, `atom_label`, `atom_number`, `name`, `shade`). Scheme compose now keeps mol caption overlays so `name` paints in schemes too.

- CoordGen and ELK are required (not features), including wasm. Product path is one Rust `render(spec) → Rendered[]`: CoordGen layout, mol-spec paint (chem + annotations + caption), then pack cells. Annotations on `MolNode` are that path, not a parallel API. `align_to` is an id/ref; Rust computes coords. JS EdgePlan host processor removed.

## 2026-10-20

- `release/v0.5.0` (and Pages/CI on main) failed `llvm-cov` at the 97% line floor. Crate coverage is 93.5% after ~20k lines of align/chrome/spokes since the last green main. Recalibrated `RUST_COV_MIN` to 93 so the gate still blocks regressions; caption / chrome_paint / layout_constraints are the remaining holes.

## 2026-10-20

- `*CC` / `CC*` spokes were collinear (centroid→CH2→CH3) because the first spoke bond was forced parallel to the stem. Dropping the SMILES dummy removed an extra vertex; the chain still needs a kink at the first carbon. First real bond is now at ±120° to the stem (π/3 turn), same as a ring ethyl; the side that misses rest atoms wins.

## 2026-10-20

- `Cl*.c1ccc(CC)cc1 |m:1:2.3.4.5.6.7|` used the benzene-style index run, so the ethyl carbons were in `m:` and spoke detect failed. CoordGen then drew `Cl*` as a separate fragment; `apply_cx_by_index` stamped a fake spoke with `atom = dummy`. Resolve `m:` onto the Relevant Ring that covers the cyclic listed atoms (drop the ethyl, complete the hexagon). Stem energy also clears rest atoms so Cl does not sit on the ethyl.

## 2026-10-20

- Spoke stem length is ring-exit along the ray plus one house bond, not centroid plus one bond. `O*.c1ccccc1 |m:1:2.3.4.5.6.7|` sat on the perimeter (ring radius ≈ 1 bond). The painted stem still runs from the centroid to the substituent; chrome `circle_clear_bonds` ignores that dummy bond so the aromatic circle is not collapsed. Clipping the stem to the ring edge was wrong.

## 2026-10-20

- Ring-center spokes from CXSMILES `m:` in `xpict-core`. Dummy `*` and the `*`-bond are dropped from the **spoke graph** before CoordGen, so `CC*` is a straight stem (centroid → CH2 → CH3 collinear) instead of an extra kink at the dummy. Rest and spoke layout separately; greedy stem along a ring-bond perp bisector; 1-D refine so the stem misses ring atoms. Fusion/bridgehead endpoints are allowed (not a 2-regular induced cycle). Molfiles / haptic `C:` still later.

## 2026-10-20

- House `strict_atoms` left aza-18-crown NH unmatched (N→O gain −1) and kinked the ring; 18-crown vs aza used to lock 18/18. N/O/S in a *shared* Relevant-Ring size now costs `COST_HETERO` (2), so NH sits on an ether O. O→C (testosterone 17-OH onto cholesterol carbon) stays refused.

## 2026-10-20

- Align tests for conjugate remodeling: quinone formation (hydroquinone→benzoquinone, APAP→NAPQI), benzene epoxidation, benzene-oxide opening (NIH-shift/hydrate), tetralin dehydrogenation. Forest single-piece products that keep the parent heavy-atom count; house maps every parent atom with matching elements.

## 2026-10-20

- Erythromycin/azithromycin is a 14→15 ring expansion with the same sugars. House was locking the sugars (24 atoms) and leaving the lactone unmatched, so CoordGen drew a snake and could flip desosamine (NMe2 vs ring-methyl). Ring seeds now require matching *exocyclic* neighbor elements (blocks that flip). House may *grow* from an equal-size seed onto disjoint ring sizes; atom-pair seeds still refuse a bare 12-cycle on a 15-cycle. Result: 48/53 mapped, NMe2→NMe2, insertion N unmatched.

## 2026-10-20

- House align was pairing the wrong atoms: MCS extended O→C (testosterone 17-OH onto a cholesterol side-chain carbon) and wrapped a 12-cycle onto a 15-cycle because bonds still paid. Fix in `xpict-core`: cyclic atoms pair only when Relevant-Ring *size sets* overlap (fusion 6+18 still maps onto a plain 18-crown; 12 vs 15 does not; chain↔ring stays free). Depict `SearchLimits::house` also sets `strict_atoms` so element-mismatch pairs are not seeded or grown; unbounded `cost_mapping` still maps N→C when that is cheapest. Degree-strict ring seeds already locked fused all-carbon rings (morphine/codeine 21-atom identity-with-skip-methyl).

## 2026-10-20

- Align ring seeds use **Relevant Rings** (union of all smallest cycles through each bond), not one SSSR cover. Cubane: SSSR 5 faces, relevant 6. Depiction (aromatic circles / interiors) still uses SSSR. Chain→ring is unchanged: no query ring means ordinary atom-pair MCS (hexane onto cyclohexane). Different-size rings are not seeded as cycles.
- House `+` align on fused pairs was scrambling the map (morphine/codeine 19 atoms, 1 identity). Seeding equal-size Relevant Rings then greedy-extend recovers the 21-atom scaffold map; CoordGen soft-constrain can still rotate the pentacycle. Simple aromatic hydroxylations overlay tightly.

## 2026-10-20

- Default `+` align on larger / macrocycle pairs (`notebooks/align_large_pairs.py`). MCS usually finds a big connected map (12–37 atoms) inside the house budget; CoordGen soft-constrain is the quality bottleneck, not the timeout. Morphine/codeine: skip already shares a pose; default *rotates* codeine (19 mapped, 151 px median). Cholesterol/testosterone: 21/21 map at 0.63 px but the cholesterol template is a bad steroid drawing, so testosterone is warped onto it. 18-crown-6 vs aza-18-crown-6 locks (18/18, 1.2 px). Size-mismatched rings (12 vs 15 crown; C12 vs C15; erythromycin vs azithromycin) map many atoms then collapse/tangle the query. Erythromycin vs clarithromycin is the only macrolide pair that stays roughly the same pose (37 mapped, 11 px).

## 2026-10-20

- Molecule align hung on reasonably sized mols: exact connected cost-mapping had no time/node caps, and `+` always stamps `align_to` so BnB ran unbounded. Escape hatches in `xpict-core` (not a host reimplementation): `AlignOpts`/`AlignToSpec` `mode` (`exact`|`greedy`|`skip`) and `timeout_ms` (`None` = house 100 ms + 50k states, `0` = unbounded). Timeout returns the greedy/BnB incumbent if ≥ `min_atoms`, else free layout. `atom_map` still skips MCS. Library `cost_mapping` stays exact/unbounded. JS/RDKit hosts honor `mode: skip` only; greedy/timeout need the in-core solver.
- Restored `+` chain-align (`align_to` previous mol). A chrome landing stub had left it off; tests still required it.
- Fitted SVG pad tracks `mol.scale` when `pad` is omitted, so `scale: 2` doubles width/height (house 4 px margin was a fixed addend).
- Region hulls cover chrome atom numbers/labels on included atoms (chem text already was). Kitchen-sink "ortho" sat outside the ring box. Lagrangian ladder unchanged — keep-off / tether / contain; this is cover membership, not energy.
- Shade disks were ~25% too large (`SHADE_FRAC` 0.90×bond). House outer radius is now 0.72×bond; atom+bond shared base stays 8/9 of that. Clash disks use the painted outer radius, so they shrink with paint.
- Containment is a third hard Lagrangian (`LAGRANGIAN_CONTAIN` = 2), after no-overlap then tether. Inside region text gets `HardConstraint::Inside` at that rank so keep-off cannot be spent to stay in the hull.
- Keep-off and tether are **different Lagrangians** (`lagrangian` 0 vs 1 on `HardConstraint`). No-overlap solves first and is held at max ρ; MaxDist cannot re-cover a number. Custom text beside an index gets a 1-bond tether. Notebook kitchen-sink test failed first (`ortho` AABB swallowed `2`), then passed.

## 2026-10-19

- Keep-off-labels is Lagrangian: prior annotation boxes (`t{10000+}`) are always hard Outside, same as chem text. A soft hinge lost to snug (kitchen-sink "ortho" on index 2).
- Solver energy is constraint data + shared geometry helpers. PreferSide edge-align is `align_along` (region 0.35 vs atom-mean; caption/callout 0) — no Point vs AABB branch. Soft/hard Inside share `inside_g`; energy is always `strength · g⁺²`. PointStandoff uses `shape_anchor`.
- Region paint cleanup: shared cover-corner / occupancy / seed helpers; emit box/circle/bezier once after snug (no placeholder patch). Dropped unused inside-of-circle seed. Tests share an ethylbenzene fixture and pin covering-circle exclude + label-shift.
- Ex6 circle (`style: circle`, inside aryl left, exclude ethyl): do not grow r about a fixed atom-centroid. Snug the label first; rebuild the **smallest enclosing circle** of atoms + label (Welzl) so the center shifts toward the text. Exclude atoms push the center, then r is recomputed. Notebook cell 14.
- Kitchen sink: paint circles + atom numbers/labels **before** region text, callouts last. Region labels hard-clear prior chrome AABBs and outside seeds may flip side when `pos` is blocked — "ring" no longer sits on exo / indices / callout.
- Ex6 inside region label: drop PreferPose + hard Inside of the grown seed hull (they pinned the label and inflated the box). Soft snug to chem; PreferSide also aligns along the edge; seed stays on `pos` (slide=0 first, no side flip); rebuild box/circle/bezier after snug. Shade omitted from inside-seed occupancy.
- Mol `name` is **not** a layout constraint. Pipeline is chrome solve → place caption clear of structure + annotations (regions/callouts/shade). Regions must not overlap the name; caption nudges along `pos` until clear. `caption_kit` remains for solver benches only.
- Region `pos` (Left/Right/Above/Below) is relative to the **mean of contained atoms**, not the hull / cover AABB or full-mol chem box. Seed slide uses that centroid; PreferSide targets a fixed `side_ref` Point at the mean.
- Inside region label snug: strong PreferPose pinned the clash-free outside seed so “aryl” never moved closer. Soft energy is now **MinimizeDist to mol/chem** (opt=clash) + PreferSide vs chem + mild PreferPose; larger NM simplex when inside. After snug, Bezier cover is **rebuilt** from atoms + final label so the outline shrinks too. Ex6 exclude test asserts nearest ring atom < 42 px and left of center.
- Bezier inside label: clash-free **outside** seed (never ring-center fallback); always add label corners to cover; grow pad so proxy surrounds atoms+label. Hull proxy defaults to **convex** (+ include-guarded exclude clip); concave dig only when an exclude sits inside the convex hull (bay). Substituent clips no longer shave ring atoms into a bottom crescent.
- Atom_label seed: `place_greedy` fallback picks the **least** `Occupancy::clash_penalty` (bond depth² weighted), not the last CW slot. Custom text also chooses preferred side via `least_clash_side` so a chem-travel alternate does not start the label across a bond.
- Bezier region paint: closed **centripetal Catmull–Rom** (α=½) → SVG cubics through the hull proxy (interpolating, cusp-free; preferred over mid-edge Q, κ-curves, or Hobby for this use). Inside labels with **excludes** no longer dump the outside-seed AABB into the cover cloud (that + half-plane clips made bowtie notches / wild handles); empty-exclude still expands cover so the hull can swallow the seed.
- Atom-label MaxDist: index/mapno use `ATOM_NUMBER_TETHER_PX` (0.3·bond); custom `atom_label` text uses longer `ATOM_TEXT_TETHER_PX` (0.5·bond). `atom_number_kit` / `refine_atom_number` take `tether_px`; chrome picks by `LabelRole`.
- Atom numbers: soft energy = strong snug `MinimizeDist` (margin) + mild soft Outside vs bonds/other labels; hard AL only for MaxDist ceiling (0.3·bond), bonds, and chem text. Snug/tether home is chem text or atom **center point** (`/snug`), not the 0.3-bond Coord disk. CCC index-1 test: snug gap + two bonds as arrow at the number. Dropped PreferPose so greedy seeds are not pinned.
- Atom-number drift fix: prior annotation boxes (`t{10000+}`) are **soft** Outside only — hard-clearing every earlier number made later indices infeasible near their atoms. Greedy `max_gap` capped at `min_gap+tether`. Post-nudge is tether-aware so bond pushes cannot fling past the home.
- Bond clearance is now a **hard** inequality via augmented Lagrangian (PHR) around Nelder–Mead — soft `Outside` hinges alone left residual ink-on-bond when PreferPose fought them. `HardConstraint::{Outside,Inside,MaxDist}`; kits push bonds (and atom-text Outside / region Inside / atom tether) into the hard set; soft PreferPose / PreferSide / shade clash stay soft. `SolveResult.max_hard_violation` reports residual `g⁺`.
- Bond clearance is **baseline** for all movable annotations: shared `push_clear_bonds` (hard Outside) on captions, region text, callouts (label/tip/routes), and atom numbers/labels. Occupancy seed (bonds + aromatic circle stroke) + refine. Circles clear non-incident bonds via `circle_clear_bonds`.
- Text halos are **paint-only** (like shadows): never a layout constraint (`Constraint::Halo` removed). After placement, chrome numbers/labels on shade get the **same glyph path** with white fill + white stroke (outline = shape + buffer), not a Minkowski blob or AABB box.
- Inside region labels (box/bezier, Ex6): seed like outside on the preferred edge, then **grow** the hull/AABB so the seed fits (`grow_aabb_to_contain` / cover corners + pad); circle already did this.
- Atom labels/numbers: **no** layout constraint vs shade. Callout arrow pullback; region Labels layer; shade clash = painted PlotDot.
- Bezier region proxy: **not** convex hull — concave dig of long chords + outward normal pad; excludes are **half-plane clipped** from the include centroid through each excluded atom so bays cannot swallow them (vertex push alone was insufficient).
- Solver bench (`solver_kit_bench`, release, 40 trials, noise=0): median **~65–110 µs** / kit; median iters **4–9** (caps 60–100); **100%** stop on pose-drift. Callout with 2 routes ~108 µs; caption ~65 µs. Re-run: `cargo test -p xpict-core --lib solver_kit_bench --release -- --nocapture`.
- Region contain: atoms are **0.3-bond disks** (not points) plus `CONTAIN_MARGIN` (0.1 bond). Circle **inside** labels seed exactly like **outside**, PreferPose holds that seed (no pull-to-center); circle grows so the seed fits, then the solver may nudge the label for clash/side.
- Callout tip: grow standoff until the tip clears chem atom-text AABBs (H of west-facing HO). Label min gap is `max(1 bond, tip_standoff + 0.5 bond)` so the tip is not glued to the callout text.
- Placement xy is a `PreferPose` spring (strength 0.35) on caption, region text, callout label/tip, route points, and atom numbers. The well sits on the paint seed; clash hinges can still spend that energy to move off an obstacle. Solver noise on those refines is 0 so the well is not kicked.
- Constraint distances: no-clash air is **0.1 CC-bond**. Name centers on **structure** (bonds + chem labels) at **0.5 bond**. Callout text stands **1 bond** off its target and avoids shade. Region/circle hulls cover atom-text boxes (OH’s H) and interior shade only (boundary atoms bonded outside the set may stick out). Region labels clear bonds and shade; outside circle labels stay within 0.5 bond of the stroke. Custom atom labels clear bonds and chem text, not shade.
- Hierarchical constraint layout landed in `doc/layout_constraints.rs`: tree under `mol/`, edge-aware Shape (Point/Circle/Line/AABB/PointSet), soft kits (caption / region text / callout / atom number+Halo), Nelder–Mead minimize with **windowed max control-point px drift** stop (not ΔE). Chrome/caption paint: greedy seed then simplex refine; bezier regions paint from `hull_proxy` → `bezier_path_from_proxy`; callouts may add adaptive route points. Progress notebook reordered easy→hard with slice legend.
- Caption layout: mol gap from typographic **ascent**; viewBox/stack from **descent** (invisible `mol-caption-metrics` frame so fit does not shrink to glyph ink). Default margin **8 px**. Flat vs descender names share the same baseline/extent.
- Region/circle stroke: `stroke: null` + fill → **no outline**; omit stroke without fill → house `#06c`; `color: "none"` / `width: 0` forces no stroke. Fill-only regions no longer pick up the default blue stroke.
- Chrome layout polish: captions snug to **ink AABB** (`CAPTION_MARGIN_DEFAULT=2`); house `SCALE` 20→**30**; notebook HTML asserts `align-items:flex-end`. Callout text+arrow share one color; tip standoff ≈ half chem label. Region labels edge-hug (inside/out); place flip does not mutate hull. New Rust `doc/layout_items` occupancy + greedy placer — region/callout migrated onto it.
- Shade `colormap`: paint is house **xenosite** only. Omit / `"xenosite"` ok; any other name (e.g. `viridis`) errors at `render_doc` via `validate_shade_colormap` — no silent ignore.
- Chrome placement bug: annotations used chem coords while `depict_molecule` paints in SVG frame space. Fix: shared `frame_offset(mol)` for circles / regions / callouts / atom labels. Circles size to enclose atom text AABB; bezier regions emit real `Q` mid-edge paths. Placement tests assert distances (arrow tip on atom, circle on atom, number near atom, hull covers atoms) — not mere existence.
- Post-layout SVG fit: after chrome + captions (and after scheme compose), `fit_scene` sets viewBox from ink AABB + house `pad` default **4 px** (`VIEWPORT_MARGIN_DEFAULT`). Optional cascading `pad` override — rarely needed. JS `sceneToSvg` now translates viewports like Python.
- Mol `name` caption polish: default font **1.5×** atom labels (`CAPTION_FONT_FRAC`); chem markup via shared `text_paint::outline_markup` (captions + region/callout chrome; no second engine). Snug place to mol AABB with `MolTextSpec.margin` (default 6 px) — minimize gap, no overlap. Rust tests in `caption` / `text_paint` / `chrome_paint`.
- Mol chrome z bands: atoms/bonds at **0**; behind negative (`background` −60 → `region` −40 → `shade` −20 → `halo` −10); in front positive with gaps (`circle` +10 → `atom_label` +20 → `callout` +40 → `name` +60). Gaps leave room for user `z` overrides.
- Wire A scaffold in `doc/chrome.rs` + `MolNode` fields (`name`, `atom_number`, `atom_label`, `region`, `circle`, `callout`); Rust-only A↔B round-trips. Privileged `name` paints via caption path (no sibling TextNode).
- Mol-chrome paint landed: `atom_number`/`atom_label`/`circle`/`region`/`callout`; aromatic circles via Marks pipeline; Chematic `mapno` on `AtomIn`; `atom_label` suppresses chem. Progress notebook: `notebooks/mol_chrome_progress.ipynb`. Open: richer FD/constraint region ladder + tighter occupancy.

- Formal charge is no longer a fake superscript: `charge_glyphs` emits Normal `+`/`−` (and magnitude digits) on the element baseline. N/S stacking already offsets the charge; the extra script lift made top charges float and bottom charges crowd quaternary N⁺. Explicit caret markup (`R^{2+}`) still superscripts.
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
