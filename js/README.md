# @swamidasslab/xpict (TypeScript)

Browser and Node client for xpict contracts, plus **shared Rust core** via WASM
(same algorithms as Python `xpict._native`). Published to **GitHub Packages** for
[xenosite.org](https://xenosite.org) — see [`docs/npm-xenosite.md`](../docs/npm-xenosite.md).

## Layout / draw (still stubs)

- **Primary layout:** Indigo WASM (stub in `src/layout/indigo-wasm.ts`)
- **Drawing:** own SVG scene (stub) — full drawer ports via `xpict-core` over time

## Native core (Rust → WASM)

```bash
# from repo root
./scripts/build_bindings.sh wasm
cd js && npm test
```

```ts
import { init, presentTemplate, draw } from "@swamidasslab/xpict";

await init(); // RDKit + xpict wasm (RDKit hidden; works in Node and browser)
presentTemplate("c1ccccc1");
const { svg, atoms } = await draw("Cc1ccccc1");
// atoms[].{x,y} are SCALE-space and match the SVG viewBox ink
```

Low-level helpers (`initNative`, `multiBondOffset`, …) remain available for
tests and Indigo paths. Shared API table: [`docs/bindings.md`](../docs/bindings.md).

**→ xenosite.org:** [`docs/migration-xenosite.md`](../docs/migration-xenosite.md)
(client-side MVP: one mol + marks + shade + align).

## Scripts

```bash
npm install
npm run check      # tsc
npm run build:wasm # wasm-pack → src/wasm/
npm run build      # wasm + tsc → dist/
npm test           # check + native smoke (tsx)
```
