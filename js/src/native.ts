/**
 * Shared Rust core bindings (wasm) — same surface as Python ``xpict._native``.
 *
 * Call ``await initNative()`` once before using exports. Rebuild with:
 * ``./scripts/build_bindings.sh wasm``
 *
 * Types are declared here so ``tsc`` works before wasm-pack has run; the
 * implementation loads ``./wasm/xpict_core.js`` produced by wasm-pack.
 */

export type ShadeDisk = {
  radiusFrac: number;
  colorZ: number;
  x: number;
  y: number;
};

type WasmModule = {
  default: (moduleOrPath?: unknown) => Promise<unknown>;
  multiBondOffset(length: number): number;
  centeredDisplacements(order: number, off: number): Float64Array | number[];
  plotdotRings(z: number, levels: number): Float64Array | number[];
  plotdotDisks(zs: number[] | Float64Array, coords: number[] | Float64Array, levels: number): Float64Array | number[];
  capsuleHaloPathD(
    x1: number,
    y1: number,
    x2: number,
    y2: number,
    inkRadius: number,
    grow: number
  ): string | undefined;
  diskHaloPathD(
    cx: number,
    cy: number,
    inkRadius: number,
    grow: number
  ): string | undefined;
  bondPx(): number;
  offsetPx(): number;
  strokePx(): number;
  shadeFrac(): number;
  coreVersion(): string;
};

let mod: WasmModule | null = null;
let initPromise: Promise<WasmModule> | null = null;

/** Load the wasm module (idempotent). Works in browser and Node. */
export async function initNative(moduleOrPath?: unknown): Promise<void> {
  if (mod) return;
  if (!initPromise) {
    initPromise = (async () => {
      const wasm = (await import("./wasm/xpict_core.js")) as WasmModule;
      let arg = moduleOrPath;
      if (arg === undefined && typeof process !== "undefined" && process.versions?.node) {
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
      await wasm.default(arg);
      return wasm;
    })();
  }
  mod = await initPromise;
}

function requireMod(): WasmModule {
  if (!mod) {
    throw new Error(
      "xpict native wasm not initialized — call await initNative() first (run ./scripts/build_bindings.sh wasm)"
    );
  }
  return mod;
}

export function isNativeReady(): boolean {
  return mod !== null;
}

export function multiBondOffset(length: number): number {
  return requireMod().multiBondOffset(length);
}

export function centeredDisplacements(order: number, off: number): number[] {
  return Array.from(requireMod().centeredDisplacements(order, off));
}

/** PlotDot rings as `{ radiusFrac, colorZ }[]`. */
export function plotdotRings(
  z: number,
  levels = 4
): Array<{ radiusFrac: number; colorZ: number }> {
  const flat = Array.from(requireMod().plotdotRings(z, levels));
  const out: Array<{ radiusFrac: number; colorZ: number }> = [];
  for (let i = 0; i + 1 < flat.length; i += 2) {
    out.push({ radiusFrac: flat[i], colorZ: flat[i + 1] });
  }
  return out;
}

/** PlotDot disks sorted weak→strong. */
export function plotdotDisks(
  zs: number[],
  coords: Array<[number, number]>,
  levels = 4
): ShadeDisk[] {
  const flatCoords: number[] = [];
  for (const [x, y] of coords) {
    flatCoords.push(x, y);
  }
  const flat = Array.from(requireMod().plotdotDisks(zs, flatCoords, levels));
  const out: ShadeDisk[] = [];
  for (let i = 0; i + 3 < flat.length; i += 4) {
    out.push({
      radiusFrac: flat[i],
      colorZ: flat[i + 1],
      x: flat[i + 2],
      y: flat[i + 3],
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
  return requireMod().capsuleHaloPathD(x1, y1, x2, y2, inkRadius, grow) ?? undefined;
}

export function diskHaloPathD(
  cx: number,
  cy: number,
  inkRadius: number,
  grow: number
): string | undefined {
  return requireMod().diskHaloPathD(cx, cy, inkRadius, grow) ?? undefined;
}

export function bondPx(): number {
  return requireMod().bondPx();
}

export function offsetPx(): number {
  return requireMod().offsetPx();
}

export function strokePx(): number {
  return requireMod().strokePx();
}

export function shadeFrac(): number {
  return requireMod().shadeFrac();
}

export function coreVersion(): string {
  return requireMod().coreVersion();
}
