# Install

Publishing setup (tokens, tags, Trusted Publishing): [Publish](publish.md).

## JavaScript

```bash
npm install @xenosite/xpict
```

Interactive demo after Pages deploy: [/js/demo/](../js/demo/).

## Python

```bash
pip install 'xpict[rdkit]'
# or from this repo:
uv sync --extra rdkit
./scripts/build_bindings.sh py
```

Native extension: `xpict-py` → `xpict._native`. RDKit is an optional layout extra.

## Rust

```toml
# Cargo.toml
xpict = "0.1"           # public API (needs system RDKit + Boost)
# or paint-only (no RDKit):
xpict-core = "0.1"
```

See [`crates/xpict/README.md`](https://github.com/swamidasslab/xenosite-pict/blob/main/crates/xpict/README.md) for system dependencies.
