/**
 * Spec-keyed SVG cache for hosts that draw the same molecules repeatedly
 * (lists, detail pages, React `use()`): one render per distinct spec, and
 * concurrent requests share the in-flight promise.
 *
 * The key is `JSON.stringify` of the depict spec, so the same SMILES with
 * different annotations is a different entry.
 */

/** Least-recently-used map of promises. */
export class SvgCache {
  #max: number;
  #entries = new Map<string, Promise<string>>();

  constructor(max = 2048) {
    this.#max = max;
  }

  get max(): number {
    return this.#max;
  }

  set max(n: number) {
    this.#max = Math.max(0, Math.floor(n));
    this.#trim();
  }

  get size(): number {
    return this.#entries.size;
  }

  /** The cached promise for `key`, or the one `make` starts. Failures are evicted. */
  get(key: string, make: () => Promise<string>): Promise<string> {
    const hit = this.#entries.get(key);
    if (hit) {
      this.#entries.delete(key);
      this.#entries.set(key, hit);
      return hit;
    }
    const p = make();
    this.#entries.set(key, p);
    p.catch(() => {
      if (this.#entries.get(key) === p) this.#entries.delete(key);
    });
    this.#trim();
    return p;
  }

  /** Settled SVG for `key` if already drawn, else `undefined` (no render). */
  peek(key: string): Promise<string> | undefined {
    return this.#entries.get(key);
  }

  clear(): void {
    this.#entries.clear();
  }

  #trim(): void {
    while (this.#entries.size > this.#max) {
      const oldest = this.#entries.keys().next().value;
      if (oldest === undefined) break;
      this.#entries.delete(oldest);
    }
  }
}
