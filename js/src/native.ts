/**
 * Internal wasm loader — one ``render(spec)`` entry for every document.
 *
 * The blob is an Emscripten module (`em++` links CoordGen's C++).
 */

import createXpict from "./wasm/xpict_core.js";
import type { InitInput, XpictWasm } from "./wasm/xpict_core.js";

let module_: XpictWasm | null = null;
let ready = false;
let initPromise: Promise<void> | null = null;

function wasmCall(fn: (ptr: number) => number, specJson: string): string {
  if (!module_) throw new Error("xpict wasm not initialized");
  const inPtr = module_.stringToNewUTF8(specJson);
  try {
    const outPtr = fn(inPtr);
    try {
      return module_.UTF8ToString(outPtr);
    } finally {
      module_._xpict_string_free(outPtr);
    }
  } finally {
    module_._free(inPtr);
  }
}

/** Product wasm entry (DepictSpec JSON → Rendered[] JSON). */
function wasmRender(specJson: string): string {
  if (!module_) {
    throw new Error("xpict wasm missing render(); rebuild with make build-wasm");
  }
  const out = wasmCall((p) => module_!._xpict_render(p), specJson);
  if (!out.startsWith("[") && !out.startsWith("{")) {
    throw new Error(out);
  }
  return out;
}

/** Load the wasm module (idempotent). Works in browser and Node. */
export async function initNative(
  moduleOrPath?: InitInput | { module_or_path: InitInput | Promise<InitInput> } | BufferSource
): Promise<void> {
  if (ready) return;
  if (!initPromise) {
    initPromise = (async () => {
      const opts: InitInput = {};
      let arg: unknown = moduleOrPath;
      if (
        arg === undefined &&
        typeof process !== "undefined" &&
        process.versions?.node
      ) {
        const { readFile } = await import("node:fs/promises");
        const { fileURLToPath } = await import("node:url");
        const { dirname, join } = await import("node:path");
        const here = dirname(fileURLToPath(import.meta.url));
        const bytes = await readFile(join(here, "wasm", "xpict_core.wasm"));
        opts.wasmBinary = new Uint8Array(bytes);
        opts.locateFile = (p) =>
          p.endsWith(".wasm") ? join(here, "wasm", "xpict_core.wasm") : p;
      } else if (arg && typeof arg === "object" && "module_or_path" in (arg as object)) {
        const inner = await (arg as { module_or_path: InitInput | Promise<InitInput> })
          .module_or_path;
        if (inner && typeof inner === "object" && "wasmBinary" in inner) {
          Object.assign(opts, inner);
        } else if (inner instanceof Uint8Array) {
          opts.wasmBinary = inner;
        } else if (typeof Buffer !== "undefined" && Buffer.isBuffer(inner)) {
          opts.wasmBinary = new Uint8Array(inner);
        }
      } else if (typeof Buffer !== "undefined" && Buffer.isBuffer(arg)) {
        opts.wasmBinary = new Uint8Array(arg);
      } else if (arg instanceof Uint8Array) {
        opts.wasmBinary = arg;
      } else if (arg && typeof arg === "object") {
        Object.assign(opts, arg as InitInput);
      }
      module_ = await createXpict(opts);
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

/** Scene JSON → SVG (Rust `scene_to_svg`; viewport ids become `data-id`). */
export function sceneToSvgJson(sceneJson: string): string {
  requireReady();
  if (!module_) {
    throw new Error("xpict wasm missing sceneToSvg(); rebuild with make build-wasm");
  }
  const out = wasmCall((p) => module_!._xpict_scene_to_svg(p), sceneJson);
  const trimmed = out.trimStart();
  if (!trimmed.startsWith("<svg")) {
    throw new Error(out);
  }
  return out;
}
