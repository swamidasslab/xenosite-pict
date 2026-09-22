# xpict browser demo (GitHub Pages)

Interactive demo: two SMILES inputs, query aligned to template, plus paint
options (`color`, `mark_atoms`, `mark_bonds`, `atom_shade`, `bond_shade`).

## Local

```bash
# from repo root
bash scripts/build_pages.sh
# default output: demo/_site — or pass a path:
# bash scripts/build_pages.sh _site
cd demo/_site && python3 -m http.server 8765
# open http://127.0.0.1:8765/
```

## Publish

Every push to `main` deploys via
[`.github/workflows/pages.yml`](../.github/workflows/pages.yml)
(also `workflow_dispatch`). Not tag-gated.

Enable **Settings → Pages → Source: GitHub Actions** once in the repo.

Live URL (after first deploy):
https://swamidasslab.github.io/xenosite-pict/
