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
export async function initNative(optsIn?: InitInput | BufferSource): Promise<void> {
  if (ready) return;
  if (!initPromise) {
    initPromise = (async () => {
      const opts: InitInput = {};
      const arg: unknown = optsIn;
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
