import { parse as parseYaml } from "yaml";
import type { DepictSpec } from "../../src/api.js";
import {
  GROUP_ORDER,
  GROUP_TITLE,
  type ReviewCase,
  type ReviewGroup,
} from "./types.js";

const PATH_RE = /(?:^|\/)cases\/([^/]+)\/([^/]+)\.(ya?ml|json)$/i;

function isGroup(value: string): value is ReviewGroup {
  return value in GROUP_TITLE;
}

function isDepictSpec(value: unknown): value is DepictSpec {
  if (!value || typeof value !== "object") return false;
  const t = (value as { type?: unknown }).type;
  return t === "mol" || t === "group" || t === "reaction_scheme" || t === "grid";
}

/**
 * One case file: YAML or JSON with `{ title?, expect?, id?, spec }`.
 * Group comes from the folder; id from the filename (override with `id`).
 */
export function parseCaseFile(filePath: string, text: string): ReviewCase {
  const norm = filePath.replace(/\\/g, "/");
  const m = PATH_RE.exec(norm);
  if (!m) {
    throw new Error(
      `case path must be cases/<group>/<id>.yaml (or .json): ${filePath}`,
    );
  }
  const folder = m[1]!;
  const stem = m[2]!;
  const ext = m[3]!.toLowerCase();
  if (!isGroup(folder)) {
    throw new Error(`unknown group folder "${folder}" in ${filePath}`);
  }
  let data: unknown;
  try {
    data = ext === "json" ? JSON.parse(text) : parseYaml(text);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    throw new Error(`${filePath}: ${msg}`);
  }
  if (!data || typeof data !== "object" || Array.isArray(data)) {
    throw new Error(`${filePath}: expected a mapping with spec`);
  }
  const rec = data as Record<string, unknown>;
  if (!isDepictSpec(rec.spec)) {
    throw new Error(`${filePath}: spec.type must be mol | group | reaction_scheme | grid`);
  }
  const id = typeof rec.id === "string" && rec.id.trim() ? rec.id.trim() : stem;
  const title =
    typeof rec.title === "string" && rec.title.trim() ? rec.title.trim() : id;
  const expect = typeof rec.expect === "string" ? rec.expect.trim() : "";
  return {
    id,
    group: folder,
    title,
    expect,
    spec: rec.spec,
    file: `cases/${folder}/${stem}.${ext === "yml" ? "yml" : ext}`,
    raw: text.replace(/^\uFEFF/, ""),
  };
}

export function sortCases(cases: ReviewCase[]): ReviewCase[] {
  const gi = new Map(GROUP_ORDER.map((g, i) => [g, i]));
  return [...cases].sort((a, b) => {
    const d = (gi.get(a.group) ?? 99) - (gi.get(b.group) ?? 99);
    if (d !== 0) return d;
    return a.id.localeCompare(b.id);
  });
}

export function assertUniqueIds(cases: ReviewCase[]): ReviewCase[] {
  const seen = new Map<string, string>();
  for (const c of cases) {
    const prev = seen.get(c.id);
    if (prev) {
      throw new Error(`duplicate case id "${c.id}": ${prev} and ${c.file}`);
    }
    seen.set(c.id, c.file);
  }
  return cases;
}
