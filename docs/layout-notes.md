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

## Strategic target: RDKit for coords+align now; native long-term; Indigo alternate

**MVP (shared Rust paint):** one molecule — Kekulé depiction, atom/bond
**marks** (circles), atom/bond **shading**, backbone/label **color**, plus
**alignment**. Multi-mol diagrams / ELK are post-MVP.

**Focus (current):** call **RDKit at each language edge** for:

1. **2D coordinates** — `backend="rdkit"` (Python `rdkit` / JS `@rdkit/rdkit`)
2. **Template alignment** — existing `align_rdkit` / MinimalLib; pass maps into Rust

**Do not** link RDKit into `xpict-core` (no Rust RDKit / no WASM chem wrapper).

**Alternate path:** **Indigo** layout + **fake/rigid align** implemented in Rust
(Kabsch / shared transforms only — no template depict).

**Long-term:** grow **native** layout+draw until the hard-case gallery is green;
RDKit remains the quality reference and the practical default while that matures.
**Chematic is out.**

Until native wins:

1. Learn algorithms from RDKit / Indigo / CoordGen / CDK (read their sources).
2. Own the **drawing** path (skeleton → offsets → wedges; our SVG), migrating into Rust.
3. Prefer **RDKit** for production-ish coords and alignment on both Python and JS.
4. Keep **Indigo** as optional alternate layout for comparison / environments without RDKit.

Shipping shared ink (fonts, geometry, bond math) as **Rust** (`xpict-core` + PyO3 +
WASM) is the dual-language path — prove in Python, then port.

**Incremental path:** workspace crate [`crates/xpict-core`](../crates/xpict-core)
holds shared pure algorithms. **Priority ports** (remove Python-only ship deps and
unify JS):

| Module | Today | Target crates | Why first |
| --- | --- | --- | --- |
| `geom` | **Done** — `i_overlay` Shape | buffer / union / xor / halo | Halos, glyph counters, annotate |
| `font` | **Done** — `ttf-parser` + Liberation | outlines / advances / Shape | Atom labels / captions → SVG paths |
| `metrics` / `plotdot` / `bonds` | Done (partial) | — | Already in core + bindings |
| `align` (rigid) | Python Kabsch | pure Rust | Indigo / no-RDKit fake align |

Capsule/disk halos and glyph outlines already call Rust. Next: WASM font
exports so the browser matches Python labels without a JS font stack.

### Alignment + coords: RDKit at the edges — never in Rust

| Layer | Who | Job |
| --- | --- | --- |
| 2D layout | **Python** `rdkit` / **JS** `@rdkit/rdkit` | `Compute2DCoords`, wedges, kekulize |
| Template align | same RDKit packages | MCS / `coordMap` template depict |
| Common math | **`xpict-core`** | Rigid Kabsch, score embeddings, apply maps — pure numbers |

Flow: language edge asks RDKit → gets atoms/bonds/coords (and align maps) →
Rust draw + optional rigid-align helpers. Indigo skip RDKit align → Rust fake align.

### Layout backends

1. **rdkit** — preferred when installed (focus now).
2. **native** — in-house; long-term product path.
3. **indigo** — alternate; pair with Rust rigid/fake align.

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

Until that checklist is green, **prefer RDKit** for demos that need trustworthy
coords; keep Indigo available as alternate; keep growing native.

## ELK for multi-mol diagrams

Metabolic networks / reaction schemes use **ELK** for viewport placement (not
chemical MCS alignment).

**Done: `elkrs` in Rust; jsrun/elkjs dropped from Python.** crates.io
[`elkrs`](https://crates.io/crates/elkrs) (ELK 0.11.0) lives in `xpict-core`
(feature `elk`). Python synthesizes ELK JSON and calls `_native.elk_layout_json`.

| Runtime | Path |
| --- | --- |
| Python | `_native.elk_layout_json` (elkrs) → row/grid fallback |
| Browser | npm `elkjs` for now; WASM builds **without** `elk` (~60 KB depict blob) |

Same ELK JSON + orthogonal edge `sections`. Coordinates follow Java ELK 0.11,
not bit-identical to elkjs 0.9. Needs **Rust ≥ 1.85**.

**Done: Shapely → `i_overlay`.** `xpict-core` feature `geom` provides `Shape`
(buffer / union / difference / even-odd contours / halo). Python draw uses
`_native.Shape`; Shapely is no longer a dependency.

`diagram.kind: reaction` widens ELK node/edge spacing and falls back to a
vertically-centered row with extra gap for arrow shafts. `layout_diagram_ex`
returns node positions **and** ELK edge section polylines (orthogonal bends);
`draw/arrows.py` strokes those routes into `Scene.overlays` (straight
viewport-to-viewport anchors when ELK is unavailable).

See also `docs/pocs/` for derisk POCs and schema-alpha feedback.
