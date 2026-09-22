# Layout & depiction notes

## Principle: depiction is hard — follow established depictors

Do **not** invent ring, chain, stereo, or collision algorithms from scratch.
Before changing layout or drawing, read what quality engines actually do and use
that as the guide:

| Concern | Primary references |
| --- | --- |
| Rings (regular / fused / spiro / bridged) | CDK `RingPlacer`, RDKit `EmbeddedFrag` / `embedRing`, CoordGen templates |
| Chains (120° zig-zag, longest chain, partner distribution) | CDK `AtomPlacer` (`placeLinearChain`, `distributePartners`), RDKit non-ring expand + collision flips |
| Bond drawing | Skeleton centerlines first; double/triple as shortened offsets (RDKit MolDraw2D, Indigo render) |
| Stereo | RDKit `WedgeMolBonds` + solid/hashed wedges; Indigo `UP`/`DOWN`/`EITHER`; E/Z via layout + crossed “either” doubles |
| Collisions / polish | RDKit flip/openAngles/shorten; CoordGen; Indigo `smart-layout` |

Hard cases (bridged cages, congested chains, stereo centers, macrocycles) are the
test of quality — not benzene.

## Strategic target: native depictor, proven — Indigo only while getting there

**If** an internal (`native`) layout + draw stack is *demonstrably* good enough,
we do not need a multi-engine layout ladder.

“Good enough” is **not** assumed. It must be shown on a growing hard-case gallery
(rings, chains, stereo, collisions) side-by-side with Indigo / RDKit / CDK-quality
*references* (read their sources; we do not ship them as layout backends).

Until that bar is cleared:

1. Learn algorithms from Indigo / RDKit / CoordGen / CDK (read their sources).
2. Own the **drawing** path now (skeleton → offsets → wedges; our SVG).
3. Grow **native layout** toward those algorithms (rings → chains → stereo placement).
4. Keep **one** transitional layout engine — **Indigo** — for real coords in POCs and
   demos. No RDKit / Open Babel / Chematic layout backends in the product path.

After native wins the gallery, Indigo shrinks to optional parse/emergency fallback.

### Chematic for perception (not depiction)

Split the problem:

| Layer | Job | Chematic? |
| --- | --- | --- |
| **Perception** | Parse, SSSR, aromaticity/Kekulé, stereo flags, valence | Strong candidate — Rust, small install, RDKit-like aromaticity (`chematic-perception`), no C++/conda |
| **2D coordinates** | Rings, chains, collisions, templates | Weak today — their depict coords suck; do not trust as publication layout |
| **Drawing** | Skeleton → offsets → wedges → SVG | **Ours** — always |

So Chematic is a plausible **default chem kernel** for perception while we own drawing and grow native layout (Indigo transitional). Using Chematic’s SVG/layout as the product surface would be a mistake.

Validate perception against RDKit on hard aromatics / stereo before promoting it; treat their `depict_*` path as throwaway.

**If** we settle on Chematic for perception, implementing **our** depiction in
**Rust** (crate next to Chematic, Python + WASM bindings) is the natural ship
stack: one molecule graph, no FFI tax between perceive and draw, small browser
bundle. Keep Python as the algorithm lab until skeleton → offsets → wedges and
the native layout quality bar are demonstrated — then port, don’t invent twice.

**Incremental path:** workspace crate [`crates/xpict-core`](../crates/xpict-core)
holds shared pure algorithms. **Priority ports** (remove Python-only ship deps and
unify JS):

| Module | Today | Target crates | Why first |
| --- | --- | --- | --- |
| `geom` | Shapely buffer/union/holes | `geo` + `i_overlay` (or Clipper) | Halos, glyph counters, annotate |
| `font` | fontTools + Liberation TTFs | `ttf-parser` / `skrifa` | Atom labels / captions → SVG paths |
| `metrics` / `plotdot` / `bonds` | Done (partial) | — | Already in core + bindings |

Capsule/disk halos already call Rust. Glyph `halo_from_shapes` / label outlines
should move next so JS never needs Shapely or fontTools.

Migrate after Python parity tests exist; do not invent depiction rules only in Rust.

### Alignment: RDKit at the language edges — never in Rust

`diagram.align` / `align_layouts` already prefers **RDKit template alignment**
when installed (`align_rdkit.py`); rigid Kabsch is the fallback.

**Do not** pull RDKit into `xpict-core` (no `rdkit-sys`, no Rust chem wrappers).
RDKit’s C++ stack does not ship a clean WASM story that way. Instead:

| Layer | Who | Job |
| --- | --- | --- |
| Call RDKit | **Python** `rdkit` / **JS** `@rdkit/rdkit` MinimalLib | Template depict, MCS / substructure matches, chem identity |
| Common math | **`xpict-core`** (PyO3 + WASM) | Rigid transforms, score embeddings, apply maps to coords — pure numbers |

Flow: language edge asks RDKit → gets atom maps + coordinates → passes arrays into
Rust helpers → gets aligned layouts back. Same Rust for both runtimes.

Do **not** add RDKit as a general layout backend ladder. Use it where it already
wins at the edges: **alignment** (and as an algorithm reference for native layout).

### Transitional layout coords

1. **Indigo** — transitional **layout** engine (2D coords) while native matures; WASM available for `js/` too.
2. **native** — must grow from stub → proven depictor; that is the real layout goal.
3. **RDKit** — **alignment only**, called from Python/JS themselves — not linked into Rust.

CoordGen / CDK remain **algorithm references**, not installed layout backends.

## Drawing model (own SVG)

Quality renders almost always:

1. Lay out as if every bond were a **single** (connected skeleton).
2. Stroke that skeleton (halo on centerlines only).
3. Add **double/triple** as shortened parallel **offsets** (ring doubles toward interior).
4. Replace stereo singles with **solid wedge** / **hashed wedge** / wavy “either”
   (thin end at stereocenter — RDKit `BEGINWEDGE` / `BEGINDASH` convention).
5. Labels and marks on top.

Do not draw three full-length equal strokes for a “triple” aromatic leak, and do
not put a halo on every offset stroke.

## Ring depiction

Quality depictors separate **coordinate generation** from **bond drawing**.
Regular polygons are produced during layout, not by post-hoc snapping of every
SSSR face in the SVG drawer.

### CDK `StructureDiagramGenerator` / `RingPlacer`

Source: `tool/sdg/.../RingPlacer.java`.

1. Find rings (SSSR / relevant rings).
2. Place an isolated ring as a regular *n*-gon.
3. Attach by shared-atom / shared-bond count:
   - **FUSED** — 2 atoms, 1 bond → remaining atoms on a regular-polygon arc.
   - **SPIRO** — 1 atom → new regular polygon around the shared atom.
   - **BRIDGED** — >2 shared atoms → only *unplaced* atoms walk an arc; the whole
     ring is **not** forced to a perfect regular polygon.
4. Place substituents after the ring system.

### RDKit Depictor / CoordGen

Sources: `Depictor/EmbeddedFrag.cpp`, `DepictUtils.cpp` (`embedRing`).

1. Each SSSR ring alone: `R = bondLen / sqrt(2*(1-cos(2π/n)))`.
2. Fused systems: stitch by shared atoms.
3. Hard systems: **ring templates** or **CoordGen**; on conflict, leave neighbors alone.

### What *xpict* does now

| Stage | Owner | Role of SSSR |
| --- | --- | --- |
| Atom coordinates | Backend today; **native layout target** | Regular / fused / bridged / template |
| Kekulé offsets, aromatic circle | Drawer (`draw/rings.py`, `scene_builder`) | Interior normals; optional circle |
| “Can all faces be regular?” | Classifier in `draw/rings.py` | FUSED/SPIRO/BRIDGED + cage (atom in ≥3 rings) |

**Do not** independently lerp every SSSR ring toward a regular polygon after a
bridged cage is laid out.

Hard-case catalog (`HARD_RING_CASES`): norbornane, cubane, adamantane, barrelene,
twistane, … — must render; must **not** claim all-regular faces.

## Chain depiction

CDK `AtomPlacer` (guide, not reinvent):

- Acyclic molecules: find **longest aliphatic chain**, place with ~**120°** bond
  angles (`placeLinearChain` / `getNextBondVector` — zig-zag, not a straight line).
- One placed neighbor → continue the chain; several unplaced neighbors →
  `distributePartners` into remaining angular space (not equal 360°/n blindly
  when some wedges are already taken by a ring).
- Chains that hit an unplaced ring atom append that atom so the attachment
  vector orients the ring system.

RDKit: expand non-ring atoms with a preferred **normal** for zig-zag; resolve
clashes by bond flips / open angles / shorten — refinement after placement.

Native layout must implement these before claiming chain quality.

## Stereochemistry

Guide: RDKit MolDraw2D wedges + `WedgeMolBonds`; Indigo `UP`/`DOWN`/`EITHER`.

- Tetrahedral: solid wedge (up) / hashed wedge (down); **narrow end at the
  stereocenter** (`begin` of the bond in layout).
- Unspecified: wavy single or crossed double (`either`).
- E/Z: primarily a **layout** problem (substituents placed correctly); drawing
  is ordinary double offsets unless “either”.

Drawing proportions follow **xenopict** ``drawer.py`` (the house style): bond
length 20 px, stroke ``0.10 × bond`` with round caps, halo ``0.20 × bond``
on every stroke, shade dots ``0.90 × bond``, atom-mark radius one bond,
font ``0.60 × bond`` (RDKit ``baseFontSize``, which xenopict does not
override). Double-bond offset stays RDKit’s ``0.15``. See ``draw/metrics.py``.
``normalize_coords`` scales each layout so the mean bond is that pixel
length (Indigo bonds are 1.0; native bonds are 1.5).

`BondLayout.stereo` carries `up`/`down`/`either`/`none`. RDKit (`WedgeMolBonds`)
and Indigo backends populate it; `draw.bonds` renders solid/hashed wedges, wavy
either singles, and crossed either doubles (thin end at `begin` = stereocenter).
Native now assigns wedges from OpenSMILES ``@``/``@@`` with a 2D parity
heuristic (not full CIP). Native also enforces E/Z from OpenSMILES ``/`` ``\\``
by flipping the smaller substituent tree across the double-bond axis after
placement (collision flips run first). CIP-accurate tetrahedral wedges remain open.

## Native quality bar (demonstration checklist)

Native is “good enough” only when hard-case demos show, without relying on Indigo
coords for those molecules:

- [x] Simple + fused aromatics (benzene, naphthalene, anthracene, phenol) — regular faces
- [x] Bridged/cage (norbornane) — no crash; no fake all-regular claim
- [x] Chains (n-alkanes, carbonyl branches) — longest-chain seed + 120° zig-zag
- [~] Congested substituents — terminal collision flip across attachment (1,2,3-Me₃Ph); deeper branch flips still open
- [~] Stereo — tetrahedral `@`/`@@` wedges (parity heuristic); E/Z from `/` `\` enforced on native coords
- [x] Side-by-side gallery vs Indigo on the same SMILES set (`poc-e-*`)

Until that checklist is green, keep **Indigo** as the transitional layout engine.
After it is green, Indigo shrinks to optional parse/emergency fallback.

## ELK for multi-mol diagrams

Metabolic networks / reaction schemes use **ELK** for viewport placement (not
chemical MCS alignment). Python runs vendored elkjs inside **jsrun** (embedded
V8) — no Node subprocess. (`mini-racer` / legacy `py-mini-racer` were tried;
elk’s fake-worker path needs `Atomics.waitAsync` plumbing that jsrun already
handles.) Grid/row stay pure-Python fallbacks. Browser `js/` can keep elkjs
directly.

`diagram.kind: reaction` widens ELK node/edge spacing and falls back to a
vertically-centered row with extra gap for arrow shafts. `layout_diagram_ex`
returns node positions **and** ELK edge section polylines (orthogonal bends);
`draw/arrows.py` strokes those routes into `Scene.overlays` (straight
viewport-to-viewport anchors when ELK is unavailable).

See also `docs/pocs/` for derisk POCs and schema-alpha feedback.
