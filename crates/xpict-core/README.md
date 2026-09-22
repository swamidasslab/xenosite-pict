# xpict-core

Shared **paint core** for [xpict](https://github.com/swamidasslab/xenosite-pict):
bonds, marks, shade, labels (with chem markup), halo, and `Scene` → used by
Python (`xpict._native`), JS/WASM (`@xenosite/xpict`), and the RDKit-backed
[`xpict`](https://crates.io/crates/xpict) crate.

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
