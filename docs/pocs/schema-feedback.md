# Schema feedback from derisk POCs (alpha)

Captured while implementing own-SVG drawing, marks/shade, and ELK placement. These are candidate contract changes — not yet baked into JSON Schema.

## Confirmed needed

1. **Stable molecule `id` (required for networks)**  
   Edges use `source`/`target` string ids. Without `MoleculeSpec.id`, layout falls back to `m{i}`, which is brittle if molecule order changes.  
   → Consider requiring `id` when `diagram.kind` is `network` or `reaction`.

2. **Shade length vs atom count**  
   Shade vectors shorter than atom count silently clip; longer silently ignore.  
   → Validator or `PictBackendWarning` when `len(shade.atoms) != n_atoms` (same for bonds).

3. **Mark atom indices are backend-order dependent**  
   Indices follow the layout backend’s atom ordering (SMILES parse order). Indigo vs RDKit can differ for the same SMILES.  
   → Document as “indices refer to `LayoutResult` for the chosen backend”; later: optional map keys / SMARTS / E-SMILES anchors.

4. **Substructure marks need a geometry hint**  
   Convex-hull pad is currently a draw constant.  
   → Optional `MarkSpec.padding` / `radius` for hull/circle sizing.

5. **Bond marks as ordered pairs**  
   `list[tuple[int,int]]` works in Python; JSON Schema should encode as length-2 arrays. Undirected bonds should match either orientation against `LayoutResult`.

## ELK / diagram

6. **ELK graph is not yet a first-class schema object**  
   We synthesize ELK JSON from molecules + `diagram.edges` + `elk_options`. That is enough for POC.  
   → Keep synthesizing; only expose raw ELK graph if callers need custom nodes (labels, ports, compounds).

7. **Edge payload is underspecified for reactions**  
   Metabolic/reaction diagrams need roles (substrate/product/enzyme), edge labels, and maybe arrowheads.  
   → Extend `DiagramEdge` with optional `label`, `kind`/`role`, and styling hooks; scene overlay draws arrows after ELK places nodes.

8. **Viewport size must precede ELK**  
   Node width/height come from normalized molecule bounds. Changing scale/padding changes ELK spacing.  
   → Either freeze draw scale as a contract constant or put `scale`/`padding` on PictSpec so layout and draw agree.

9. **Python ELK path** — **done via jsrun**  
   ELK runs through vendored elkjs inside **jsrun** (embedded V8). No Node on the
   host. Prefer jsrun over `mini-racer`/`py-mini-racer` (elk fake-worker /
   `Atomics.waitAsync` gaps). Grid/row remain fallbacks. Do not put
   `engine: elkjs|jar` in PictSpec.

## Drawing / scene (trickle-back)

10. **Native depictor vs multi-backend; Chematic for perception**  
    Depiction is hard — always review CDK / RDKit / Indigo / CoordGen before changing layout or draw code.  
    Product path: **native** layout+draw proven on the hard-case gallery; **Indigo only** as transitional layout. Chematic is a strong candidate for **perception** (aromaticity, SSSR, stereo flags — small Rust, RDKit-like) but **not** for publication coords (their depictions suck). Own the SVG.  
    **If** we settle on Chematic for perception, a **Rust depiction crate** (Python + WASM) is the coherent ship stack — shared molecule graph, small browser bundle. Prove algorithms in the Python POC first; do not bake a multi-engine layout ladder into PictSpec. See `docs/layout-notes.md`.

11. **Ring / chain / stereo algorithms are depictor-owned practice**  
    Regular polygons, 120° zig-zag chains, partner distribution, wedges/hashes belong in layout (native target, backends for now). The drawer: skeleton centerlines → offset doubles/triples → stereo wedges. Bridged/cage systems cannot have every SSSR face forced regular — `HARD_RING_CASES`. Do not add a PictSpec field that promises “all rings regular.”

12. **Stereo wedges** — **done (drawing)**  
    `BondLayout.stereo` + `draw.bonds` solid/hashed wedges / either wavy / crossed
    doubles (RDKit thin-at-begin). Native layout still owes CIP-consistent wedge
    *assignment*; backends already supply stereo for transitional coords.

13. **Halo / label collision**  
    Bond shortening into heteroatom labels is a start; aromatic rings and overlapping shade dots need more policy.  
    → Optional `MoleculeSpec.halo` already helps; consider `label_style` later.

14. **HTML composition**  
    `diagram.kind: html` is a stub hook. Multi-SVG pages need a document model (sections, captions) beyond a flat viewport list.

## Explicitly deferred (do not expand schema yet)

- Chemical MCS alignment options  
- Full CXSMILES / E-SMILES feature parity fields  
- Colormap registry beyond a string name  
- Per-layer CSS theme objects
