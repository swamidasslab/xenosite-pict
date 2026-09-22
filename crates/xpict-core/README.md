# xpict-core

Shared **depiction algorithms** for xpict. Python (`src/xpict/`) remains the
algorithm lab and the working engine. As pieces stabilize, move them here so
Python (PyO3) and JS (WASM) share one implementation.

## What belongs here

| Module | Role | Python today |
| --- | --- | --- |
| `metrics` | Bond / stroke / offset fractions | `draw/metrics.py` |
| `plotdot` | Concentric shade disks (xenopict) | `draw/plotdot.py` |
| `bonds` | Multi-bond offset helpers | `draw/bonds.py` (partial) |
| `elk` | Multi-mol diagram placement (elkrs) | `diagram/elk.py` |
| `geom` | Buffer / union / counters (`i_overlay` Shape) | `draw/halo.py`, `glyphs.py` |
| `font` *(stub)* | Liberation outlines + advances | `draw/font_face.py`, `glyphs.py` |
| `rings` *(stub)* | SSSR helpers, interior normals | `draw/rings.py` |

**Not** in this crate (yet or ever as invent-your-own): chem layout backends
(Indigo/RDKit wrappers) or PictSpec JSON parsing — those stay at the language
edges. **ELK** (`elkrs`) and **geom** (`i_overlay` Shape) are in-core features.

## Migration rule

1. Prove behavior in Python tests / gallery.
2. Port the pure function(s) + a Rust unit test that mirrors the Python assert.
3. Optionally call Rust from Python once parity is boring.
4. WASM bindings last (same `rlib`).

Do not invent new depiction rules in Rust that Python does not already own.

## Build

```bash
cargo test -p xpict-core
cargo clippy -p xpict-core -- -D warnings
```

## Python (PyO3) and JS (WASM)

| Edge | Crate | Consumer |
| --- | --- | --- |
| Python | `crates/xpict-py` → `xpict._native` | `src/xpict/native_bridge.py` |
| JS | `crates/xpict-wasm` → `js/src/wasm/` | `js/src/native.ts` |

```bash
./scripts/build_bindings.sh all
```

See [`docs/bindings.md`](../../docs/bindings.md). Keep both binding crates in sync when
adding exports.

**Priority:** `font` (Liberation outlines) next so Python can drop fontTools
and JS shares the same labels.

**Not in this crate:** RDKit or any chem engine. Python/JS call RDKit themselves
for alignment; Rust only gets numeric maps/coordinates.

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
