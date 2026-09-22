# @xenosite/xpict (JS / TypeScript)

Browser and Node client for xpict contracts, plus **shared Rust core** via WASM
(same algorithms as Python `xpict._native`).

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
import { initNative, multiBondOffset, plotdotRings } from "@xenosite/xpict";

await initNative();
multiBondOffset(20); // 3
plotdotRings(1.0);   // [{ radiusFrac, colorZ }, ...]
```

Shared API table: [`docs/bindings.md`](../docs/bindings.md).

## Scripts

```bash
npm install
npm run check      # tsc
npm run build:wasm # wasm-pack → src/wasm/
npm run build      # wasm + tsc → dist/
npm test           # check + native smoke
```
