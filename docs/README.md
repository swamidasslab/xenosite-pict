# Documentation (MkDocs Material)

Cross-language docs are built with **[MkDocs Material](https://squidfunk.github.io/mkdocs-material/)**:

| Source | Role |
| --- | --- |
| `mkdocs.yml` | Site config + nav |
| `docs/` | Markdown pages, gallery assets, styles |
| `scripts/build_pages.sh` | Gallery SVGs + TypeDoc + rustdoc + `mkdocs build` + JS demo |
| `scripts/generate_doc_examples.py` | Regenerates `docs/assets/examples/*.svg` |

Autodoc:

- **Python** — [mkdocstrings](https://mkdocstrings.github.io/) (`docs/api/python.md`)
- **JavaScript** — [TypeDoc](https://typedoc.org/) → `docs/typedoc/` (iframe on API page)
- **Rust** — `cargo doc -p xpict-core` → `docs/rustdoc/`

The old hand-rolled `site/*.html` tree is retired; do not add pages there.

## Local

```bash
uv sync --group docs --extra rdkit
bash scripts/build_pages.sh
uv run --group docs mkdocs serve
# http://127.0.0.1:8000/
```

## Publish

Every push to `main` deploys via [`.github/workflows/pages.yml`](../.github/workflows/pages.yml).

Live: https://swamidasslab.github.io/xenosite-pict/
