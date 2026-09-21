# @xenosite/xpict (JS / TypeScript)

Browser and Node processor for the same JSON contracts as Python `xpict`.

## Layout

- **Primary:** Indigo WASM (`indigo-ketcher` / Indigo wasm) — stub in `src/layout/indigo-wasm.ts`
- **Secondary (optional later):** `@rdkit/rdkit` MinimalLib
- Drawing always happens here (`src/draw/svg.ts`), never by mutating engine SVG

## Shared schema

JSON Schema is generated from Pydantic and committed under `../schema/` (`xpict.schema.json`, etc.).

## Scripts

```bash
npm install
npm run check   # tsc --noEmit
npm run build   # emit dist/
```

Python remains the working engine today; this package validates the shared API surface.
