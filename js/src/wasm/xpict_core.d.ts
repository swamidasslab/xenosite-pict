/** Emscripten factory options (subset we pass). */
export type InitInput = {
  wasmBinary?: Uint8Array;
  locateFile?: (path: string, prefix: string) => string;
};

export interface XpictWasm {
  _xpict_render(ptr: number): number;
  _xpict_scene_to_svg(ptr: number): number;
  _xpict_string_free(ptr: number): void;
  UTF8ToString(ptr: number): string;
  stringToNewUTF8(s: string): number;
  _free(ptr: number): void;
}

declare function createXpict(opts?: InitInput): Promise<XpictWasm>;
export default createXpict;
