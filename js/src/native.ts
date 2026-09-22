/**
 * Internal wasm loader for the MVP paint path.
 * Not part of the package public API — used by ``api.ts`` only.
 */

import init, {
  depictMolecule as wasmDepictMolecule,
  type InitInput,
} from "./wasm/xpict_core.js";

let ready = false;
let initPromise: Promise<void> | null = null;

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

export function isNativeReady(): boolean {
  return ready;
}

/** MVP paint: `MoleculeIn` JSON → `Scene` JSON. */
export function depictMolecule(moleculeJson: string): string {
  if (!ready) {
    throw new Error("xpict wasm not initialized");
  }
  return wasmDepictMolecule(moleculeJson);
}
