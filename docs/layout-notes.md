# Layout notes

## Backend preference

1. **Indigo** — preferred publication-quality 2D coords (also has WASM for `js/`).
2. **RDKit** — strong default; good CXSMILES support. Prefer **CoordGen** + ring templates when available (`rdDepictor.SetPreferCoordGen(True)`, `useRingTemplates=True`).
3. **Open Babel / pybel** — broad format coverage; solid 2D fallback.
4. **Chematic** — last resort; coordinate quality is weaker.
5. **native** — toy stub for tests without chem engines.

Own pure-Python layout engine is deferred: Indigo’s layout alone is large (~17k LOC C++). Wrap first.

## Ring depiction (do not invent)

Quality depictors separate **coordinate generation** from **bond drawing**. Regular polygons are produced during layout, not by post-hoc snapping of every SSSR face in the SVG drawer.

### CDK `StructureDiagramGenerator` / `RingPlacer`

Source: `tool/sdg/.../RingPlacer.java`.

1. Find rings (SSSR / relevant rings).
2. Place an isolated ring as a regular *n*-gon (`getNativeRingRadius`, equal angles).
3. Attach further rings by shared-atom / shared-bond count:
   - **FUSED** — 2 atoms, 1 bond → place remaining atoms on a regular-polygon arc outside the shared bond.
   - **SPIRO** — 1 atom → new regular polygon around the shared atom.
   - **BRIDGED** — >2 shared atoms → `placeBridgedRing`: only *unplaced* atoms walk an arc (`tStep = tRemain / (numUnplaced+1)`). The whole ring is **not** forced to a perfect regular polygon.
4. Place substituents after the ring system is placed.

### RDKit Depictor / CoordGen

Sources: `Depictor/EmbeddedFrag.cpp`, `DepictUtils.cpp` (`embedRing`).

1. Each SSSR ring alone: regular polygon with  
   `R = bondLen / sqrt(2*(1-cos(2π/n)))`.
2. Fused systems: stitch by shared atoms (transform next ring onto already-embedded atoms).
3. Hard systems: match **ring templates**, else **CoordGen**; when stitching conflicts (bridged cages), neighbors are left alone rather than over-constrained into regular *n*-gons.

### What *xenosite.pict* does

| Stage | Owner | Role of SSSR |
| --- | --- | --- |
| Atom coordinates | Layout backend (Indigo / RDKit+CoordGen / …) | Regular / fused / bridged / template placement |
| Kekulé offsets, aromatic circle | Drawer (`draw/rings.py`, `scene_builder`) | Interior normals; optional aromatic circle geometry |
| “Can all faces be regular?” | Classifier in `draw/rings.py` | CDK FUSED/SPIRO/BRIDGED + cage heuristic (atom in ≥3 rings) |

**Do not** independently lerp every SSSR ring toward a regular polygon after the backend has laid out a bridged cage — that fights CDK/RDKit/Indigo.

### Hard cases (not all rings regular)

Catalog in `HARD_RING_CASES` / tests:

- **Bridged** (CDK BRIDGED): norbornane, norbornadiene, bicyclo[2.2.2]octane, barrelene, twistane.
- **Cages** (pairwise FUSED yet overconstrained; atom in ≥3 rings): cubane, adamantane.

These must still render cleanly; they must **not** be forced into all-regular faces.

## ELK for multi-mol diagrams

Metabolic networks, reaction schemes, and grids use **ELK** for viewport placement (not chemical MCS alignment).

- **POC (now):** `js/pocs/elk_layout.mjs` + Node subprocess from `diagram/elk.py` when `diagram.kind` is `network` or `reaction`. Install with `npm install --prefix js`. Falls back to row layout with `PictBackendWarning`.
- **Target:** Python ships ELK JAR + V8 (no Node required on every host). Web keeps elkjs.
- Grid/row remain pure-Python for `grid` / default multi-mol.

Chemical MCS alignment (xenopict-style) is a separate optional pass before diagram layout.

See also `docs/pocs/` for derisk POCs and schema-alpha feedback.
