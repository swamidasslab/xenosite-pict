# Install

## JavaScript

```bash
npm install @xenosite/xpict
```

Interactive align demo: [js/demo/](../js/demo/).

## Python

```bash
pip install 'xpict[rdkit]'
```

RDKit is the optional layout backend. Without it, a native toy layout is used
for smoke / non-chem runs.

## Rust

```toml
# Cargo.toml
xpict = "0.1"           # public API (needs system RDKit + Boost)
# or paint-only (no RDKit):
xpict-core = "0.1"
```

System deps for the native `xpict` crate:
[`crates/xpict/README.md`](https://github.com/swamidasslab/xenosite-pict/blob/main/crates/xpict/README.md).
