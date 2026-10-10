# Contributing

PRs and bug reports are welcome.

## One Rust core — zero wrapper drift (hard rule)

**JS, Python, and Rust wrappers expose the same APIs and the same behavior
because they all use `xpict-core`.** There is no expected per-language drift
to maintain by hand. If two wrappers disagree, a host has duplicated logic —
fix by routing through Rust, not by patching a second implementation.

When you add or change a customer-facing surface (`mol` / `render` /
`to_svg` / `depict`, options, paint), update the **Rust core** and the thin
bindings/types for each language in the same change. Do not leave a language
on a stub or a parallel engine.

| Surface | JS | Python | Rust |
| --- | --- | --- | --- |
| Single molecule | `xpict.mol` / `render` / `toSvg` | `mol` / `render` / `to_svg` | `mol` / `render` / `to_svg` |
| Declarative document | `xpict.depict` | `depict` / `render(doc)` | `depict` |

| Layer | Owns |
| --- | --- |
| `xpict-core` (Rust) | All product semantics (paint, layout, align, cascade, compose, edge plan, …) |
| Python / JS | Generated types + adapters only (bindings, IPython/DOM, host I/O, packaging) |

## Where to file what

| Topic | Where |
| --- | --- |
| Bugs, install failures, wrong depiction | [GitHub Issues](https://github.com/swamidasslab/xenosite-pict/issues) |
| Pull requests | [GitHub PRs](https://github.com/swamidasslab/xenosite-pict/pulls) |
| Nested documents / reactions / ELK / full `PictSpec` | Comment on the **future** design — [`python/xpict/future/`](python/xpict/future/README.md) and [`schema/future/xpict.schema.json`](schema/future/xpict.schema.json); open an issue and link those paths |

Please prefer a small issue or PR over a large speculative redesign of the shipped `mol` / `render` / `toSvg` API.

## Development

```bash
make help              # documented targets (build / test / publish helpers)
make agent-install     # uv sync + maturin develop
make test              # Rust core + Python + JS
make pages             # MkDocs site → _site/
```

Equivalent without Make: see the `Makefile` recipes (they wrap
`scripts/build_bindings.sh`, `uv`, `cargo`, and `npm`).

Label markup: [`docs/label-markup.md`](docs/label-markup.md).
Publish / tags: [`.github/PUBLISH.md`](.github/PUBLISH.md).
Cloud / coding agents: [`AGENTS.md`](AGENTS.md).

## License

MIT — see [`LICENSE`](LICENSE).
