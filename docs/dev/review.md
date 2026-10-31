# Local review gallery

A private Vite site that paints **live `DepictSpec` through the JS/WASM
library**. It is not deployed with Pages.

```bash
make review
```

Opens `http://127.0.0.1:5177`. Figures come from `xpict.depict` /
`xpict.toSvg` (same paint as `@xenosite/xpict`).

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

Each card has ok / problem / later plus a textarea. Stored in
`localStorage`; export / import JSON from the header.

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
