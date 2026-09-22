/**
 * Profile JS xpict.render pipeline stages.
 * Run: `npx tsx scripts/profile_pipeline.ts`
 */
import { performance } from "node:perf_hooks";
import { xpict } from "../src/api.js";
import { ensureRdkit } from "../src/rdkit-loader.js";
import { initNative, depictMolecule } from "../src/native.js";
import { layoutWithRdkit } from "../src/layout/rdkit-layout.js";
import { sceneToSvg } from "../src/draw/scene-svg.js";
import type { Scene } from "../src/draw/scene-svg.js";

const CASES = [
  { id: "benzene", source: "c1ccccc1" },
  { id: "phenol", source: "c1ccccc1O" },
  { id: "aspirin", source: "CC(=O)Oc1ccccc1C(=O)O" },
  { id: "star_cx", source: "*C1C=C(O)C=CC1=O |$GSH;;;;;;;;$|" },
  { id: "multi_star", source: "*C* |$;;R2;$|" },
];

const WARMUP = 3;
const ITERS = 25;

function ms(t0: number, t1: number): number {
  return t1 - t0;
}

function stats(samples: number[]): { mean: number; p50: number; p95: number; min: number; max: number } {
  const s = [...samples].sort((a, b) => a - b);
  const mean = s.reduce((a, b) => a + b, 0) / s.length;
  const p50 = s[Math.floor(s.length * 0.5)]!;
  const p95 = s[Math.floor(s.length * 0.95)]!;
  return { mean, p50, p95, min: s[0]!, max: s[s.length - 1]! };
}

function fmt(n: number): string {
  return n.toFixed(2);
}

async function profileCase(source: string) {
  // Stage timers (after warmup of full path)
  const stages = {
    layout: [] as number[],
    depict: [] as number[],
    toSvg: [] as number[],
    full: [] as number[],
  };

  for (let i = 0; i < WARMUP; i++) {
    await xpict.render(xpict.mol(source));
  }

  for (let i = 0; i < ITERS; i++) {
    const tFull0 = performance.now();

    const t0 = performance.now();
    const laid = await layoutWithRdkit(source);
    const t1 = performance.now();

    const molJson = JSON.stringify(laid.molecule);
    const t2 = performance.now();
    const sceneJson = depictMolecule(molJson);
    const t3 = performance.now();

    const scene = JSON.parse(sceneJson) as Scene;
    const t4 = performance.now();
    sceneToSvg(scene);
    const t5 = performance.now();

    stages.layout.push(ms(t0, t1));
    stages.depict.push(ms(t2, t3));
    stages.toSvg.push(ms(t4, t5));
    stages.full.push(ms(tFull0, t5));
  }

  // Also full public API (includes opts / frame cache)
  const fullApi: number[] = [];
  for (let i = 0; i < ITERS; i++) {
    const t0 = performance.now();
    const r = await xpict.render(xpict.mol(source));
    xpict.toSvg(r.scene);
    fullApi.push(performance.now() - t0);
  }

  return { stages, fullApi };
}

async function main() {
  const tInit0 = performance.now();
  await Promise.all([ensureRdkit(), initNative()]);
  const initMs = performance.now() - tInit0;

  console.log(JSON.stringify({ kind: "js_init_ms", initMs }, null, 0));
  console.log(
    [
      "case".padEnd(12),
      "layout".padStart(8),
      "depict".padStart(8),
      "toSvg".padStart(8),
      "staged".padStart(8),
      "api".padStart(8),
      "(mean ms, n=" + ITERS + ")",
    ].join(" ")
  );

  const rows: Record<string, unknown>[] = [];
  for (const c of CASES) {
    const { stages, fullApi } = await profileCase(c.source);
    const L = stats(stages.layout);
    const D = stats(stages.depict);
    const S = stats(stages.toSvg);
    const F = stats(stages.full);
    const A = stats(fullApi);
    console.log(
      [
        c.id.padEnd(12),
        fmt(L.mean).padStart(8),
        fmt(D.mean).padStart(8),
        fmt(S.mean).padStart(8),
        fmt(F.mean).padStart(8),
        fmt(A.mean).padStart(8),
      ].join(" ")
    );
    rows.push({
      id: c.id,
      layout_ms: L,
      depict_ms: D,
      toSvg_ms: S,
      staged_full_ms: F,
      public_api_ms: A,
    });
  }

  const out = { kind: "js_pipeline_profile", initMs, iters: ITERS, warmup: WARMUP, rows };
  const { writeFileSync, mkdirSync } = await import("node:fs");
  mkdirSync("/opt/cursor/artifacts", { recursive: true });
  writeFileSync(
    "/opt/cursor/artifacts/js_pipeline_profile.json",
    JSON.stringify(out, null, 2)
  );
  console.log("wrote /opt/cursor/artifacts/js_pipeline_profile.json");
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
