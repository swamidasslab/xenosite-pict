import { readdir, readFile } from "node:fs/promises";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { assertUniqueIds, parseCaseFile, sortCases } from "./parse.js";
import type { ReviewCase } from "./types.js";

const REVIEW_ROOT = dirname(fileURLToPath(new URL(".", import.meta.url)));
export const CASES_DIR = join(REVIEW_ROOT, "cases");

async function walk(dir: string): Promise<string[]> {
  const out: string[] = [];
  for (const ent of await readdir(dir, { withFileTypes: true })) {
    const p = join(dir, ent.name);
    if (ent.isDirectory()) out.push(...(await walk(p)));
    else out.push(p);
  }
  return out;
}

/** Load every `cases/<group>/<id>.yaml` (Node / smoke / Playwright). */
export async function loadCasesFromDisk(): Promise<ReviewCase[]> {
  const files = (await walk(CASES_DIR)).filter((p) =>
    /\.(ya?ml|json)$/i.test(p),
  );
  const cases: ReviewCase[] = [];
  for (const abs of files) {
    const rel = relative(REVIEW_ROOT, abs);
    const text = await readFile(abs, "utf8");
    cases.push(parseCaseFile(rel, text));
  }
  return sortCases(assertUniqueIds(cases));
}
