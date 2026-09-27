# xpict-core

Shared **paint core** for [xpict](https://github.com/swamidasslab/xenosite-pict):
**declarative molecule depiction** rendered as **publication-quality vector
graphics** (SVG paths). Used from Rust, Python, and JS/WASM.

| Capability | Status |
| --- | --- |
| Kekulé bonds, wedges, marks, shade, color | Shipped |
| Atom labels + chem markup (`$R_1$`, `R^2`, `\alpha`, bold/italic) | Shipped |
| Halo / glyph outlines (Liberation Sans) | Shipped |
| Document two-pass (`plan_edge` / `render_doc`) + CX chrome | Shipped |
| House MCS ([`mcs_atom_map`](https://docs.rs/xpict-core), chematic 1.0.27) | Shipped — discovery |
| Chematic 2D (`chematic-layout` + host flag) | Experimental — not schema |
| Multi-mol / ELK / reactions | Shipped (`compose_scheme`) |

**No RDKit** in this crate. Default 2D coords stay at language edges
(RDKit Depictor / MinimalLib). Opt-in chematic coords via feature
``chematic-layout`` + host ``chematic_layout`` / ``XPICT_CHEMATIC_LAYOUT``
(not DepictSpec). Chematic MCS / SMARTS are always on.

```bash
cargo test -p xpict-core
cargo clippy -p xpict-core -- -D warnings
```

Docs: [bindings](https://github.com/swamidasslab/xenosite-pict/blob/main/docs/bindings.md) ·
[label markup](https://github.com/swamidasslab/xenosite-pict/blob/main/docs/label-markup.md) ·
[GitHub Pages](https://swamidasslab.github.io/xenosite-pict/)

## Contributing

Bug reports and PRs:
https://github.com/swamidasslab/xenosite-pict  

Comments on the future nested document model:
https://github.com/swamidasslab/xenosite-pict/tree/main/python/xpict/future  

## License

MIT
