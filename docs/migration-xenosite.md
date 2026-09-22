# Migration path: xpict on xenosite.org (client-side)

**Goal:** replace server-side xenopict SVGs on [xenosite.org](https://xenosite.org)
with **`@xenosite/xpict` in the browser**, ASAP.

**MVP product:** one molecule — Kekulé depiction, atom/bond **circles**, atom/bond
**shading**, backbone/label **color**, **alignment**. Not multi-mol / ELK scenes.

```
  @rdkit/rdkit (coords + template align)
           │
           ▼
  xpict-core.wasm  →  Scene (JSON / typed)
           │
           ▼
  thin JS: Scene → SVG → data:image/svg+xml <img>
           │
           ▼
  xenosite interactive-molecule (existing CSS)
```

Python stays the algorithm lab and CI truth; the **ship path for xenosite is WASM**.

---

## Where we are

| Piece | Status |
| --- | --- |
| `SCALE=20`, metrics, plotdot rings/disks | ✅ Rust + Py + WASM |
| Capsule / disk halo paths | ✅ Rust + Py + WASM |
| Label place + backbone insets | ✅ Rust + Py (WASM font feature next) |
| Bond offset helpers | ✅ Rust + Py + WASM |
| Bond strokes / joins / wedges | ✅ Rust (+ Py/WASM via `depict_molecule`) |
| Mark circles (atoms/bonds) | ✅ Rust `depict_molecule` marks layer |
| Shade paint onto scene | ✅ Rust (simple coral LUT; full xenosite cmap TBD) |
| `Scene` document types | ✅ Rust (`xpict-core::scene`) |
| `depict_molecule(MoleculeIn) → Scene` | ✅ Rust + Py + WASM (bonds/marks/shade; labels/halo next) |
| JS Scene → SVG / data-URI `<img>` | ❌ stub (`layoutToSvgStub`) |
| Browser RDKit layout + align | ❌ not in this repo (xenosite may already load MinimalLib) |
| Multi-mol / ELK | post-MVP (ignore for this track) |

xenosite today already embeds depictions as **data-URI `<img>`** with
`width`/`height` = viewBox and `.interactive-molecule__img { max-width:100%; height:auto }`
— that host CSS stays; we only swap the SVG bytes.

---

## Path (ordered for ASAP)

### Phase A — Browser can paint one mol from coords *(unblock xenosite)*

1. **Rust `depict_molecule`** (single viewport `Scene`)
   - Input: `MoleculeIn` (atoms/bonds in SVG/`SCALE` space, optional
     `atom_shade` / `bond_shade` / `mark_atoms` / `mark_bonds`).
   - Port from Python in this order (each with core tests mirroring pytest):
     1. bond strokes + centered joins (your join rule: doubles extend onto singles)
     2. labels + halo (reuse `labels` + `geom` + `font`)
     3. mark circles
     4. shade disks (`plotdot` → circle primitives)
   - Output: `Scene` (serde JSON is the ABI).

2. **WASM export** `depictMolecule(json) → json` (and/or typed wasm-bindgen).

3. **Thin JS serializer** `sceneToSvg(scene)` + `sceneToImgDataUri(scene)`
   - Match Python: unitless `width`/`height` = viewBox; `data:image/svg+xml;utf8,…`.
   - Drop `layoutToSvgStub`.

4. **xenosite spike:** RDKit MinimalLib → coords → `depictMolecule` → `<img class="interactive-molecule__img">`.
   - No PictSpec required for the spike; pass `MoleculeIn` directly.
   - Prove: acetone + SoM shade + one atom circle + aligned pair.

**Exit:** xenosite can render a shaded/marked mol client-side without xenopict.

### Phase B — Align + package for production

5. **Alignment at the edge**
   - Prefer RDKit template align in JS (same as Python `align_rdkit`).
   - Add Rust rigid/Kabsch helper for Indigo / no-RDKit fallbacks; WASM-export it.

6. **Publish `@xenosite/xpict`** (wasm + `sceneToSvg` + `initNative`) for the
   xenosite app bundler. Keep API: `await initNative(); depict(molIn) → img src`.

7. **Parity CI:** Python golden SVGs (or Scene JSON) vs WASM for a small set:
   ethanol, acetone, phenol, one shaded SoM, one marked atom.

**Exit:** xenosite.org ships client depict for the All Models molecule card.

### Phase C — Retire Python draw on the ship path *(can lag)*

8. Python `drawable` / `bonds` call into `_native.depict_molecule` once parity is boring.
9. Glyph/halo only via Rust (Shapely/fontTools off the ship path) — already partly done.
10. Multi-mol / ELK only after single-mol is live on xenosite.

---

## Non-goals (do not block A/B)

- Reaction schemes, networks, ELK, HTML multi-figure pages  
- Copying xenosite’s double-bond V-tip (we use extend-onto-singles)  
- Native 2D layout quality for hard cases (RDKit remains coords source)  
- Putting RDKit inside `xpict-core`

---

## Suggested first PR slice after this doc

`depict_molecule` in Rust that returns bonds + labels + halo for an unlabeled
alkane / acetone (no shade/marks yet), WASM + JS `sceneToSvg`, acetone demo in
`js/` that matches Python Scene JSON for the same coords.

Then add shade + marks (what xenosite SoM pages need) before general cleanup.
