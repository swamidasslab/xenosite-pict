/**
 * Internal wasm loader — one ``render(spec)`` entry for every document.
 */

import init, * as wasmBindings from "./wasm/xpict_core.js";
import type { InitInput } from "./wasm/xpict_core.js";

/** Product wasm entry (DepictSpec JSON → Rendered[] JSON). */
function wasmRender(specJson: string): string {
  const fn = (wasmBindings as { render?: (s: string) => string }).render;
  if (typeof fn !== "function") {
    throw new Error("xpict wasm missing render(); rebuild with make build-wasm");
  }
  return fn(specJson);
}

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

function structureFields(source: string): {
  smiles?: string;
  cxsmiles?: string;
  molfile?: string;
} {
  const text = source.trim();
  if (text.includes("V2000") || text.includes("V3000") || text.split("\n").length >= 4) {
    return { molfile: text };
  }
  if (text.includes("|")) return { cxsmiles: text };
  return { smiles: text };
}

/** Wrap a lone mol (source / `{smiles}` / MolNode) as a DepictSpec. */
export function specFromSingleMol(raw: unknown): Record<string, unknown> {
  if (typeof raw === "string") {
    return { type: "mol", ...structureFields(raw) };
  }
  if (!raw || typeof raw !== "object") {
    throw new Error("depictMolecule: expected mol source or DepictSpec");
  }
  const o = raw as Record<string, unknown>;
  const t = o.type;
  if (t === "mol" || t === "group" || t === "reaction_scheme" || t === "grid") {
    return o;
  }
  if (typeof o.source === "string") {
    return { type: "mol", ...structureFields(o.source) };
  }
  if (o.smiles || o.cxsmiles || o.molfile) {
    return { type: "mol", ...o };
  }
  throw new Error("depictMolecule: expected mol source or DepictSpec");
}

/**
 * Single mol → spec → {@link renderJson}.
 * Accepts a source string, `{ source }`, `{ smiles|cxsmiles|molfile }`, or a DepictSpec.
 */
export function depictMolecule(molJson: string): string {
  requireReady();
  const spec = specFromSingleMol(JSON.parse(molJson) as unknown);
  return wasmRender(JSON.stringify(spec));
}

/** DepictSpec JSON → `Rendered[]` JSON (layout + mol-spec paint + pack). */
export function renderJson(specJson: string): string {
  requireReady();
  return wasmRender(specJson);
}
