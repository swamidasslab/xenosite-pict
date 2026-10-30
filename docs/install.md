# Install

## JavaScript

```bash
npm install @xenosite/xpict
```

Interactive align demo: [js/demo/](../js/demo/).

## Python

```bash
pip install xpict
```

Layout is native (SMILES/molfile parse + CoordGen in `xpict-core`). No RDKit extra.

## Rust

```toml
# Cargo.toml
xpict = "0.1"           # public API (CoordGen; no system RDKit)
# or paint-only:
xpict-core = "0.1"
```

Native crate notes:
[`crates/xpict/README.md`](https://github.com/swamidasslab/xenosite-pict/blob/main/crates/xpict/README.md).
