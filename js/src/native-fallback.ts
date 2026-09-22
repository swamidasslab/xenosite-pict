/**
 * Thin JS fallbacks mirroring ``xpict-core`` when wasm is not built.
 * Prefer ``initNative()`` + exports from ``./native.js`` in production.
 */

export function multiBondOffsetPy(length: number, offsetPx = 3.0): number {
  if (length < 2.0 * offsetPx) {
    return Math.min(offsetPx, length * 0.25);
  }
  return offsetPx;
}

export function plotdotRingsPy(
  z: number,
  levels = 4
): Array<{ radiusFrac: number; colorZ: number }> {
  if (Math.abs(z) < 0.05) return [];
  const stops = Array.from({ length: levels }, (_, i) => (i + 1) / levels);
  const radius = (level: number): number => {
    const az = Math.abs(z);
    if (level === 0) return Math.sqrt(stops[0]);
    const offset = 1 - stops[level];
    const r = az - offset;
    return r < stops[0] ? 0 : Math.sqrt(r);
  };
  const color = (level: number): number => {
    const sign = z < 0 ? -1 : 1;
    return level === 0 ? z : sign * stops[stops.length - level - 1];
  };
  const out: Array<{ radiusFrac: number; colorZ: number }> = [];
  for (let lvl = 0; lvl < levels; lvl++) {
    const r = radius(lvl);
    if (r > 0) out.push({ radiusFrac: r, colorZ: color(lvl) });
  }
  return out;
}
