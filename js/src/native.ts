/**
 * Shared Rust core bindings (wasm) — same surface as Python ``xpict._native``.
 *
 * Call ``await initNative()`` once before using exports. Rebuild with:
 * ``./scripts/build_bindings.sh wasm``
 */

import init, {
  bondPx as wasmBondPx,
  capsuleHaloPathD as wasmCapsuleHaloPathD,
  centeredDisplacements as wasmCenteredDisplacements,
  coreVersion as wasmCoreVersion,
  depictMolecule as wasmDepictMolecule,
  diskHaloPathD as wasmDiskHaloPathD,
  multiBondOffset as wasmMultiBondOffset,
  offsetPx as wasmOffsetPx,
  plotdotDisks as wasmPlotdotDisks,
  plotdotRings as wasmPlotdotRings,
  shadeFrac as wasmShadeFrac,
  strokePx as wasmStrokePx,
  type InitInput,
} from "./wasm/xpict_core.js";

export type { InitInput };

export type ShadeDisk = {
  radiusFrac: number;
  colorZ: number;
  x: number;
  y: number;
};

let ready = false;
let initPromise: Promise<void> | null = null;

function toF64(values: ArrayLike<number>): Float64Array {
  return values instanceof Float64Array ? values : Float64Array.from(values);
}

/** Load the wasm module (idempotent). Works in browser and Node. */
export async function initNative(
  moduleOrPath?: InitInput | { module_or_path: InitInput | Promise<InitInput> }
): Promise<void> {
  if (ready) return;
  if (!initPromise) {
    initPromise = (async () => {
      let arg: InitInput | { module_or_path: InitInput | Promise<InitInput> } | undefined =
        moduleOrPath;
      if (
        arg === undefined &&
        typeof process !== "undefined" &&
        process.versions?.node
      ) {
        // --target web uses fetch(); Node needs bytes or a Response.
        const { readFile } = await import("node:fs/promises");
        const { fileURLToPath } = await import("node:url");
        const { dirname, join } = await import("node:path");
        const here = dirname(fileURLToPath(import.meta.url));
        const bytes = await readFile(join(here, "wasm", "xpict_core_bg.wasm"));
        arg = { module_or_path: bytes };
      } else if (
        arg !== undefined &&
        typeof Buffer !== "undefined" &&
        Buffer.isBuffer(arg)
      ) {
        arg = { module_or_path: arg };
      } else if (arg instanceof Uint8Array) {
        arg = { module_or_path: arg };
      }
      await init(arg);
      ready = true;
    })();
  }
  await initPromise;
}

function requireReady(): void {
  if (!ready) {
    throw new Error(
      "xpict native wasm not initialized — call await initNative() first (run ./scripts/build_bindings.sh wasm)"
    );
  }
}

export function isNativeReady(): boolean {
  return ready;
}

export function multiBondOffset(length: number): number {
  requireReady();
  return wasmMultiBondOffset(length);
}

export function centeredDisplacements(order: number, off: number): number[] {
  requireReady();
  return Array.from(wasmCenteredDisplacements(order, off));
}

/** PlotDot rings as `{ radiusFrac, colorZ }[]`. */
export function plotdotRings(
  z: number,
  levels = 4
): Array<{ radiusFrac: number; colorZ: number }> {
  requireReady();
  const flat = Array.from(wasmPlotdotRings(z, levels));
  const out: Array<{ radiusFrac: number; colorZ: number }> = [];
  for (let i = 0; i + 1 < flat.length; i += 2) {
    out.push({ radiusFrac: flat[i]!, colorZ: flat[i + 1]! });
  }
  return out;
}

/** PlotDot disks sorted weak→strong. */
export function plotdotDisks(
  zs: number[],
  coords: Array<[number, number]>,
  levels = 4
): ShadeDisk[] {
  requireReady();
  const flatCoords: number[] = [];
  for (const [x, y] of coords) {
    flatCoords.push(x, y);
  }
  const flat = Array.from(
    wasmPlotdotDisks(toF64(zs), toF64(flatCoords), levels)
  );
  const out: ShadeDisk[] = [];
  for (let i = 0; i + 3 < flat.length; i += 4) {
    out.push({
      radiusFrac: flat[i]!,
      colorZ: flat[i + 1]!,
      x: flat[i + 2]!,
      y: flat[i + 3]!,
    });
  }
  return out;
}

export function capsuleHaloPathD(
  x1: number,
  y1: number,
  x2: number,
  y2: number,
  inkRadius: number,
  grow: number
): string | undefined {
  requireReady();
  return wasmCapsuleHaloPathD(x1, y1, x2, y2, inkRadius, grow) ?? undefined;
}

export function diskHaloPathD(
  cx: number,
  cy: number,
  inkRadius: number,
  grow: number
): string | undefined {
  requireReady();
  return wasmDiskHaloPathD(cx, cy, inkRadius, grow) ?? undefined;
}

/** MVP paint: `MoleculeIn` JSON → `Scene` JSON. */
export function depictMolecule(moleculeJson: string): string {
  requireReady();
  return wasmDepictMolecule(moleculeJson);
}

export function bondPx(): number {
  requireReady();
  return wasmBondPx();
}

export function offsetPx(): number {
  requireReady();
  return wasmOffsetPx();
}

export function strokePx(): number {
  requireReady();
  return wasmStrokePx();
}

export function shadeFrac(): number {
  requireReady();
  return wasmShadeFrac();
}

export function coreVersion(): string {
  requireReady();
  return wasmCoreVersion();
}
