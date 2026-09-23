/**
 * Internal wasm loader — paint + document two-pass.
 * Document: ``planEdge`` → host ``processEdgePlan`` → ``renderDoc``.
 * Chrome (CX / star / shade) is applied in core ``renderDoc``.
 */

import init, {
  depictMolecule as wasmDepictMolecule,
  planEdge as wasmPlanEdge,
  renderDoc as wasmRenderDoc,
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

function requireReady(): void {
  if (!ready) {
    throw new Error("xpict wasm not initialized");
  }
}

/** MVP paint: `MoleculeIn` JSON → `Scene` JSON. */
export function depictMolecule(moleculeJson: string): string {
  requireReady();
  return wasmDepictMolecule(moleculeJson);
}

/** Pass 1: DepictSpec JSON → EdgePlan JSON (or null). */
export function planEdgeJson(specJson: string): string {
  requireReady();
  return wasmPlanEdge(specJson);
}

/** Pass 2: DepictSpec + EdgeResult JSON → DocPaint list JSON. */
export function renderDocJson(specJson: string, edgeJson: string): string {
  requireReady();
  return wasmRenderDoc(specJson, edgeJson);
}
