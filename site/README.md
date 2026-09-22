# xpict docs site (GitHub Pages)

Cross-language documentation. Source of the Pages deploy:

| Path | Content |
| --- | --- |
| `site/` | Static docs (home, API, install, JS / Python / Rust) |
| `demo/` | Interactive JS align demo → published at **`/js/demo/`** |

## Local

```bash
bash scripts/build_pages.sh
cd demo/_site && python3 -m http.server 8765
# http://127.0.0.1:8765/          docs home
# http://127.0.0.1:8765/js/demo/  JS demo
```

## Publish

Every push to `main` deploys via [`.github/workflows/pages.yml`](../.github/workflows/pages.yml).

Enable **Settings → Pages → Source: GitHub Actions** once.

Live: https://swamidasslab.github.io/xenosite-pict/
