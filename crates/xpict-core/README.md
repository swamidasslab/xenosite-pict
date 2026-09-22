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
| `geom` *(stub)* | Buffer / union / counters (Shapely) | `draw/halo.py`, `glyphs.py` |
| `font` *(stub)* | Liberation outlines + advances | `draw/font_face.py`, `glyphs.py` |
| `rings` *(stub)* | SSSR helpers, interior normals | `draw/rings.py` |

**Not** in this crate (yet or ever as invent-your-own): layout backends,
Indigo/RDKit wrappers, ELK, or PictSpec JSON parsing — those stay at the
language edges.

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

## Python (PyO3)

Bindings live in `crates/xpict-py` → import `xpict._native` after:

```bash
uv sync --group dev
maturin develop --manifest-path crates/xpict-py/Cargo.toml
```

Python calls Rust via `xpict.native_bridge` (capsule/disk halos, plotdot,
bond offset). **Glyphs** still use fontTools + Shapely; only simple ink uses
Rust geometry today.

FFI features (`wasm-bindgen`) will be additive; the default `rlib` stays
dependency-light.

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
