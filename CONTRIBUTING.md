# Contributing

PRs and bug reports are welcome.

## Where to file what

| Topic | Where |
| --- | --- |
| Bugs, install failures, wrong depiction | [GitHub Issues](https://github.com/swamidasslab/xenosite-pict/issues) |
| Pull requests | [GitHub PRs](https://github.com/swamidasslab/xenosite-pict/pulls) |
| Nested documents / reactions / ELK / full `PictSpec` | Comment on the **future** design — [`python/xpict/future/`](python/xpict/future/README.md) and [`schema/future/xpict.schema.json`](schema/future/xpict.schema.json); open an issue and link those paths |

Please prefer a small issue or PR over a large speculative redesign of the shipped `mol` / `render` / `toSvg` API.

## Development

```bash
./scripts/build_bindings.sh all
uv sync --extra rdkit
uv run pytest -q
cd js && npm test
cargo test -p xpict-core
```

Versioning and publish tags: [`docs/publish.md`](docs/publish.md).  
Label markup: [`docs/label-markup.md`](docs/label-markup.md).

## License

MIT — see [`LICENSE`](LICENSE).
