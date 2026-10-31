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

The page is one scrolling tree: expand/collapse all, groups only, or a
single group. Figures paint lazily when a case is open and in view.

Notes (ok / problem / later) autosave to `notes.local.json` (gitignored)
and `localStorage`. Export / import JSON still works from the header. On
reload, problem cases sort first; editing notes does not reshuffle.

## Visual diffs

`make review-test` snapshots `.review-figure` for each case (`?only=<id>`).
Update baselines with:

```bash
cd js && npx playwright test --config review/playwright.config.ts --update-snapshots
```
