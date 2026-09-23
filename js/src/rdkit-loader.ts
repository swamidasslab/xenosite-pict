/**
 * Hidden RDKit MinimalLib bootstrap.
 *
 * Callers never import `@rdkit/rdkit` themselves: {@link ensureRdkit} pulls
 * the npm module in Node/bundlers, or injects a `<script>` in the browser.
 */

export type RdkitMol = {
  is_valid(): boolean;
  has_coords(): number;
  delete(): void;
  get_json(): string;
  get_coords(): number[][] | [];
  get_molblock(): string;
  get_num_atoms(): number;
  get_num_bonds(): number;
  convert_to_kekule_form(): void;
  set_new_coords(): boolean;
  set_new_coords(useCoordGen: boolean): boolean;
  normalize_depiction(): number;
  generate_aligned_coords(templateMol: RdkitMol, options: string): string;
  get_substruct_match(templateMol: RdkitMol): string;
  get_substruct_matches(templateMol: RdkitMol): string;
};

export type RdkitMolList = {
  append(mol: RdkitMol): void;
};

export type RdkitModule = {
  version(): string;
  get_mol(input: string, details_json?: string): RdkitMol | null;
  get_qmol(smarts: string): RdkitMol | null;
  MolList: new () => RdkitMolList;
  get_mcs_as_json(mols: RdkitMolList, details_json?: string): string;
  get_mcs_as_smarts(mols: RdkitMolList, details_json?: string): string;
  get_mcs_as_mol(mols: RdkitMolList, details_json?: string): RdkitMol | null;
};
export type RdkitLoadOptions = {
  /** Browser: URL of `RDKit_minimal.js` (default: unpkg matching our dep). */
  scriptUrl?: string;
  /** Override wasm path resolution for the RDKit module. */
  locateFile?: (file: string) => string;
};

const DEFAULT_SCRIPT =
  "https://unpkg.com/@rdkit/rdkit@2026.3.6/dist/RDKit_minimal.js";

let modulePromise: Promise<RdkitModule> | null = null;
let rdkit: RdkitModule | null = null;

function isBrowser(): boolean {
  return typeof document !== "undefined" && typeof window !== "undefined";
}

function injectScript(src: string): Promise<void> {
  const existing = document.querySelector<HTMLScriptElement>(
    `script[data-xpict-rdkit="1"]`
  );
  if (existing) {
    if ((window as unknown as { initRDKitModule?: unknown }).initRDKitModule) {
      return Promise.resolve();
    }
    return new Promise((resolve, reject) => {
      existing.addEventListener("load", () => resolve());
      existing.addEventListener("error", () =>
        reject(new Error(`RDKit script failed: ${existing.src}`))
      );
    });
  }
  return new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.src = src;
    script.async = true;
    script.dataset.xpictRdkit = "1";
    script.onload = () => resolve();
    script.onerror = () => reject(new Error(`RDKit script failed to load: ${src}`));
    document.head.appendChild(script);
  });
}

function defaultLocateFile(scriptUrl: string): (file: string) => string {
  const slash = scriptUrl.lastIndexOf("/");
  const base = slash >= 0 ? scriptUrl.slice(0, slash + 1) : "";
  return (file: string) => `${base}${file}`;
}

type InitFn = (opts?: { locateFile?: (file: string) => string }) => Promise<RdkitModule>;

async function loadInitFn(opts: RdkitLoadOptions): Promise<InitFn> {
  if (isBrowser()) {
    const w = window as unknown as { initRDKitModule?: InitFn };
    if (!w.initRDKitModule) {
      await injectScript(opts.scriptUrl ?? DEFAULT_SCRIPT);
    }
    if (!w.initRDKitModule) {
      throw new Error("RDKit script loaded but initRDKitModule is missing");
    }
    return w.initRDKitModule;
  }
  const mod = (await import("@rdkit/rdkit")) as { default: InitFn };
  return mod.default;
}

/** Idempotent: load + init RDKit. Safe to call from {@link init}. */
export async function ensureRdkit(opts: RdkitLoadOptions = {}): Promise<RdkitModule> {
  if (rdkit) return rdkit;
  if (!modulePromise) {
    modulePromise = (async () => {
      const initFn = await loadInitFn(opts);
      const locateFile =
        opts.locateFile ??
        (isBrowser() ? defaultLocateFile(opts.scriptUrl ?? DEFAULT_SCRIPT) : undefined);
      const mod = locateFile ? await initFn({ locateFile }) : await initFn();
      rdkit = mod;
      return mod;
    })().catch((err) => {
      modulePromise = null;
      throw err;
    });
  }
  return modulePromise;
}

export function isRdkitReady(): boolean {
  return rdkit !== null;
}

/** Test helper — clear cached module (does not unload the script). */
export function _resetRdkitForTests(): void {
  rdkit = null;
  modulePromise = null;
}
