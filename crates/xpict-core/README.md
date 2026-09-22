# xpict-core

Shared **depiction algorithms** for xpict. Python (`python/xpict/`) remains the
algorithm lab and the working engine. As pieces stabilize, move them here so
Python (PyO3) and JS (WASM) share one implementation.

## MVP (focus)

**One molecule at a time** — not multi-mol diagrams / ELK scenes.

| Capability | Status |
| --- | --- |
| Standard Kekulé depiction (skeleton, doubles/triples, wedges, labels, halo) | Python working; Rust bonds ✅ + `depict_molecule` (bonds/marks/shade); labels/halo TBD |
| Circling atoms / bonds (publication marks) | Rust `depict_molecule` marks layer ✅ |
| Shading atoms / bonds (plot-dot disks) | Rust `plotdot` + `depict_molecule` shade (simple coral LUT) ✅ |
| Backbone / label ink color (`MoleculeSpec.color`) | ✅ Python + Rust paint; on `MoleculeIn` ABI |
| Alignment | RDKit template align at language edges; Rust rigid/fake align for Indigo |

Caller supplies SVG-space coords (+ optional shade scores / mark indices).
Core returns a [`Scene`](src/scene.rs) with **one viewport**. Python/JS only
serialize to SVG / data-URI `<img>`.

**Out of MVP:** reaction/network diagrams, ELK placement, multi-viewport
overlays. `elk` stays in-tree for later; do not block the depict path on it.

## What belongs here

| Module | Role | Python today |
| --- | --- | --- |
| `metrics` | Bond / stroke / offset fractions (`SCALE=20`) | `draw/metrics.py` |
| `plotdot` | Concentric shade disks (xenopict) | `draw/plotdot.py` |
| `bonds` | Strokes, stereo, centered joins | `draw/bonds.py` |
| `depict` | `MoleculeIn` → single-viewport `Scene` | `draw/drawable.py` (partial) |
| `geom` | Buffer / union / counters (`i_overlay` Shape) | `draw/halo.py`, `glyphs.py` |
| `font` | Liberation outlines + advances (`ttf-parser`) | `draw/font_face.py`, `glyphs.py` |
| `labels` | Atom-label orientation + backbone insets | `draw/label_place.py` |
| `scene` | Drawable document (primitives → SVG) | `contracts/scene.py` |
| `rings` *(stub)* | SSSR helpers, interior normals | `draw/rings.py` |
| `elk` *(post-MVP)* | Multi-mol diagram placement | `diagram/elk.py` |

**Not** in this crate: chem layout backends (Indigo/RDKit) or PictSpec JSON
parsing — those stay at the language edges. Python/JS call RDKit for 2D coords
and template alignment; Rust only gets numbers (plus rigid align helpers).

## Migration rule

1. Prove behavior in Python tests / gallery.
2. Port the pure function(s) + a Rust unit test that mirrors the Python assert.
3. Optionally call Rust from Python once parity is boring.
4. WASM bindings last (same `rlib`).

Do not invent new depiction rules in Rust that Python does not already own.

**MVP port order:** labels + halo onto `depict_molecule` → thin serializers →
parity CI. Bonds / marks / shade already in core.

```bash
cargo test -p xpict-core
cargo clippy -p xpict-core -- -D warnings
./scripts/rust_coverage.sh          # llvm-cov; CI floor is 97% lines
```

JS and Python CI jobs **`needs: [rust]`** — Pages / publish call the same workflow.

## Python (PyO3) and JS (WASM)

| Edge | Crate | Consumer |
| --- | --- | --- |
| Python | `crates/xpict-py` → `xpict._native` | `python/xpict/native_bridge.py` |
| JS | `crates/xpict-wasm` → `js/src/wasm/` | `js/src/native.ts` |

```bash
./scripts/build_bindings.sh all
```

See [`docs/bindings.md`](../../docs/bindings.md) and
[`docs/migration-xenosite.md`](../../docs/migration-xenosite.md) (ASAP path to
xenosite.org client-side). Keep both binding crates in sync when adding exports.

**Not in this crate:** RDKit or any chem engine.

## LLM-assisted ports (no Rust required to start)

You do not need to author Rust by hand. A workable loop:

1. Point the agent at a **small, pure** Python module (`plotdot.py`, `metrics.py`,
   offset helpers) plus its pytest.
2. Ask for a Rust port **into the matching `xpict-core` module**, with unit tests
   that mirror the Python asserts (same numbers).
3. You only need to run: `cargo test -p xpict-core` (and keep Python tests green).
4. Do **not** ask for a wholesale `drawable.py` / layout dump in one shot.
5. Reject ports that invent new depiction rules; Python gallery remains truth.

Review the diff for: `unsafe`, new deps you did not ask for, and behavior that
is not covered by a test. Clippy (`-D warnings`) catches a lot of footguns.
