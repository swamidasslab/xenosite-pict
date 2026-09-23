# Contributing

PRs and bug reports are welcome.

## Language alignment (hard rule)

**JS, Python, and Rust ship the same public APIs.** When you add or change a
customer-facing surface (`mol` / `render` / `to_svg` / `depict`, options like
`align_to` / `star_labels`, paint behaviour), update **all three** languages
plus docs and tests in the same change. Do not leave a language on a stub or
“not yet” note.

| Surface | JS | Python | Rust |
| --- | --- | --- | --- |
| Single molecule | `xpict.mol` / `render` / `toSvg` | `mol` / `render` / `to_svg` | `mol` / `render` / `to_svg` |
| Declarative document | `xpict.depict` | `depict` / `render(doc)` | `depict` |

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
