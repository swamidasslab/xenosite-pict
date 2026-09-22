# xpict-core

Shared **paint core** for [xpict](https://github.com/swamidasslab/xenosite-pict):
**declarative molecule depiction** rendered as **publication-quality vector
graphics** (SVG paths). Used from Rust, Python, and JS/WASM.

**No RDKit** in this crate — layout stays at language edges.

| Capability | Status |
| --- | --- |
| Kekulé bonds, wedges, marks, shade, color | Shipped |
| Atom labels + chem markup (`$R_1$`, `R^2`, `\alpha`, bold/italic) | Shipped |
| Halo / glyph outlines (Liberation Sans) | Shipped |
| Multi-mol / ELK / reactions | Design only (`xpict.future`) |

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
