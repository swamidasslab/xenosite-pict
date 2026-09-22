/* tslint:disable */
/* eslint-disable */

export function bondPx(): number;

export function capsuleHaloPathD(x1: number, y1: number, x2: number, y2: number, ink_radius: number, grow: number): string | undefined;

export function centeredDisplacements(order: number, off: number): Float64Array;

export function coreVersion(): string;

/**
 * `MoleculeIn` JSON → `Scene` JSON (MVP paint ABI).
 */
export function depictMolecule(molecule_json: string): string;

export function diskHaloPathD(cx: number, cy: number, ink_radius: number, grow: number): string | undefined;

export function multiBondOffset(length: number): number;

export function offsetPx(): number;

/**
 * Flat `[radius, color, x, y, …]` sorted weak→strong.
 */
export function plotdotDisks(zs: Float64Array, coords: Float64Array, levels: number): Float64Array;

/**
 * Flat `[radius, color, …]` for one score (empty when near zero).
 */
export function plotdotRings(z: number, levels: number): Float64Array;

export function shadeFrac(): number;

export function strokePx(): number;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly bondPx: () => number;
    readonly capsuleHaloPathD: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly centeredDisplacements: (a: number, b: number) => [number, number];
    readonly coreVersion: () => [number, number];
    readonly depictMolecule: (a: number, b: number) => [number, number, number, number];
    readonly diskHaloPathD: (a: number, b: number, c: number, d: number) => [number, number];
    readonly multiBondOffset: (a: number) => number;
    readonly offsetPx: () => number;
    readonly plotdotDisks: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly plotdotRings: (a: number, b: number) => [number, number];
    readonly shadeFrac: () => number;
    readonly strokePx: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
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
