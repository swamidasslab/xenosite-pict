# xpict browser demo (GitHub Pages)

Interactive demo: two SMILES inputs, query aligned to template, plus paint
options (`color`, `mark_atoms`, `mark_bonds`, `atom_shade`, `bond_shade`).

## Local

```bash
# from repo root
bash scripts/build_pages.sh
cd demo/_site && python3 -m http.server 8765
# open http://127.0.0.1:8765/
```

## Publish

Pushes to `main` that touch `demo/`, `js/`, or the wasm crates deploy via
[`.github/workflows/pages.yml`](../.github/workflows/pages.yml).

Enable **Settings → Pages → Source: GitHub Actions** once in the repo.

Live URL (after first deploy):
https://swamidasslab.github.io/xenosite-pict/
