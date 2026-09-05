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

## Strategic target: native depictor, proven — not five backends forever

**If** an internal (`native`) layout + draw stack is *demonstrably* good enough,
the need for multiple chem layout backends is greatly diminished.

“Good enough” is **not** assumed. It must be shown on a growing hard-case gallery
(rings, chains, stereo, collisions) side-by-side with Indigo / RDKit / CDK-quality
references. Until that bar is cleared, optional engines remain scaffolding:

1. Learn algorithms from Indigo / RDKit / CoordGen / CDK (read their sources).
2. Own the **drawing** path now (skeleton → offsets → wedges; our SVG).
3. Grow **native layout** toward those algorithms (rings → chains → stereo placement).
4. Retire the backend preference ladder once native wins the gallery on hard cases.

After that, chem libraries shrink to optional roles: SMILES/molfile **parse**,
CXSMILES extras, or emergency fallback — not a permanent multi-engine product.

### Chematic for perception (not depiction)

Split the problem:

| Layer | Job | Chematic? |
| --- | --- | --- |
| **Perception** | Parse, SSSR, aromaticity/Kekulé, stereo flags, valence | Strong candidate — Rust, small install, RDKit-like aromaticity (`chematic-perception`), no C++/conda |
| **2D coordinates** | Rings, chains, collisions, templates | Weak today — their depict coords suck; do not trust as publication layout |
| **Drawing** | Skeleton → offsets → wedges → SVG | **Ours** — always |

So Chematic is a plausible **default chem kernel** for perception while we own drawing and grow native (or Indigo/RDKit) layout. Using Chematic’s SVG/layout as the product surface would be a mistake.

Validate perception against RDKit on hard aromatics / stereo before promoting it; treat their `depict_*` path as throwaway.

### Transitional layout coords (scaffolding only)

1. **Indigo** — current best coords while native matures (WASM for `js/` too).
2. **RDKit** (+ CoordGen / ring templates) — strong reference and fallback.
3. **Open Babel / pybel** — format breadth.
4. **Chematic** — perception OK; **coords last resort only**.
5. **native** — must grow from toy stub → proven depictor; that is the real goal.

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

### What *xenosite.pict* does now

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

`BondLayout.stereo` carries `up`/`down`/`either`/`none`. Backends should populate
it; the drawer must render it before claiming publication quality. Native layout
must assign wedges consistently with CIP/coords, not invent a new convention.

## Native quality bar (demonstration checklist)

Native is “good enough” only when hard-case demos show, without relying on Indigo
coords for those molecules:

- [ ] Simple + fused aromatics (benzene, naphthalene, indole) — regular faces, interior doubles
- [ ] Bridged/cage (norbornane, cubane, adamantane) — no crash; no fake all-regular claim
- [ ] Chains (n-alkanes, branched) — 120° zig-zag, sensible branching
- [ ] Congested substituents — no catastrophic overlap (or documented refinement)
- [ ] Stereo (`C[C@H](O)Cl`, E/Z alkenes) — correct wedge/hash and double geometry
- [ ] Side-by-side gallery vs Indigo/RDKit on the same SMILES set

Until that checklist is green, keep optional backends. After it is green, drop
them from the default path.

## ELK for multi-mol diagrams

Metabolic networks / reaction schemes use **ELK** for viewport placement (not
chemical MCS alignment). POC: `js/pocs/` + `diagram/elk.py`. Target: JAR+V8
without Node on every host. Grid/row stay pure-Python.

See also `docs/pocs/` for derisk POCs and schema-alpha feedback.
