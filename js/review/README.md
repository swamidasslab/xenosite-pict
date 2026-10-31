# Local depiction review

Private Vite app. Paints every case through **this checkout’s**
`@xenosite/xpict` (`file:..` → `js/`, Vite alias to `js/src`). Never the
npm registry. Not part of the published package or the public MkDocs site.

```bash
make review          # http://127.0.0.1:5177
npm run test:catalog # paint every case in Node
make review-test     # Playwright PNG diffs (chromium)
```

## Cases

Examples live in [`cases/`](cases/) as YAML (one file per figure). The app
loads that directory; it does not hard-code structures. See
[`cases/README.md`](cases/README.md).

Notes (ok / problem / later) stay in `localStorage` and can be exported as
JSON from the header.

## Visual diffs

`make review-test` snapshots `.review-figure` for each case (`?only=<id>`).
Update baselines with:

```bash
cd js && npx playwright test --config review/playwright.config.ts --update-snapshots
```
