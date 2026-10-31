# Local review gallery

A private Vite site that paints **live `DepictSpec` through the JS/WASM
library**. It is not deployed with Pages.

```bash
make review
```

Opens `http://127.0.0.1:5177`. Figures come from `xpict.depict` /
`xpict.toSvg` on the **in-repo** JS/WASM tree (`js/review` depends on
`@xenosite/xpict` via `file:..`; Vite aliases that to `js/src`). It never
installs the published npm package.

## Cases (the source of truth)

Do **not** hard-code examples in the React app. Add a YAML file:

```
js/review/cases/<group>/<id>.yaml
```

```yaml
title: Ethanol
expect: Open-chain carbon skeleton with an OH label.
spec:
  type: mol
  smiles: CCO
```

`<group>` is the sidebar section; `<id>` is the snapshot / notes key (unique
across groups). Format: [`js/review/cases/README.md`](../../js/review/cases/README.md).
`region/` covers hull styles, exclude, padding, GSH cover, and z-order;
`scheme/` covers coterminal lanes, fans, wraps, and multi-endpoint nets.

`npm run test:catalog` (part of `make test-js`) paints every file.

## Notes

Cases sit in a collapsible tree (group → case). **Expand all** to scroll
the catalog; **Groups** to fold figures and keep section headers; **Collapse
all** to start from a closed tree. Each group also has Expand / Collapse for
just that branch. Figures paint when a case is open and near the viewport.

Each open case has ok / problem / later plus a textarea. Autosaves to
`js/review/notes.local.json` (gitignored) and mirrors `localStorage`.
The last committed problem list is `js/review/problems.json` (id → note).
Export / import JSON still works from the header. On reload, groups and
cases with **problem** notes sort first (then **later**); marking a case
while the page is open does not reshuffle.

## Visual regression

Playwright snapshots each `.review-figure` (`make review-test`). Needs
Chromium once: `cd js && npx playwright install chromium`. Not part of the
default `make test` (browser download). Catalog smoke still runs in CI.

Checked-in baselines are macOS (`*-darwin.png`). Linux CI will not match them
until a linux set is added; run visual tests locally on this machine.

## Layout

| Path | Role |
| --- | --- |
| `js/review/cases/` | DepictSpec YAML |
| `js/review/Depict.tsx` | Thin React wrapper around `xpict.depict` |
| `js/review/App.tsx` | Gallery chrome + notes |
| `js/review/catalog/` | YAML loader (disk for Node, glob for Vite) |
