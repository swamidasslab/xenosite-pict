/* tslint:disable */
/* eslint-disable */

/**
 * `MoleculeIn` JSON → `Scene` JSON (MVP paint ABI).
 */
export function depictMolecule(molecule_json: string): string;

/**
 * Pass 1: `DepictSpec` JSON → `EdgePlan` JSON (or ``null`` when empty).
 */
export function planEdge(spec_json: string): string;

/**
 * Pass 2: `DepictSpec` + `EdgeResult` JSON → DocPaint list JSON.
 */
export function renderDoc(spec_json: string, edge_json: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly depictMolecule: (a: number, b: number, c: number) => void;
    readonly planEdge: (a: number, b: number, c: number) => void;
    readonly renderDoc: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
