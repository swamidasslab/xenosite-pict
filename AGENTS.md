# Agent notes

Prefer **`make`** for build / test / publish helpers. Run `make help` for the
full list.

| Task | Command |
| --- | --- |
| Bootstrap (Python + native ext) | `make agent-install` |
| Tests (Rust core + Python + JS) | `make test` |
| Live types from Rust | `make types` / `make types-check` |
| Docs site | `make sync-docs && make pages` |
| Version bump (product) | `make bump VERSION=X.Y.Z` |
| Registry publish | Tag-driven — see [`.github/PUBLISH.md`](.github/PUBLISH.md); local dry-runs via `make publish-dry-*` |

**Cloud Agent install:** set the environment `install` command to
`make agent-install` (keeps dashboard snapshot / Dockerfile; do not commit a
minimal `.cursor/environment.json` that would override it).

Language wrappers share one Rust core — see [`CONTRIBUTING.md`](CONTRIBUTING.md).

## One Rust core — zero wrapper drift (hard rule)

**No expected behavioral drift between JS / Python / Rust wrappers:** they
must call the same `xpict-core` implementation. Hosts are types +
language-specific adapters only (bindings, IPython/DOM display, Chematic
I/O, packaging). Product semantics live exclusively in Rust.

If a host reimplements cascade, paint, layout, align, compose, or DepictSpec
rules, that is a bug — move it into Rust, do not “align” a second copy.
