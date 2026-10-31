import { assertUniqueIds, parseCaseFile, sortCases } from "./parse.js";
import type { ReviewCase } from "./types.js";

const raw = import.meta.glob("../cases/**/*.{yaml,yml,json}", {
  eager: true,
  query: "?raw",
  import: "default",
}) as Record<string, string>;

/** Vite/browser: every case file under `review/cases/`. */
export function loadCasesFromGlob(): ReviewCase[] {
  const cases = Object.entries(raw).map(([path, text]) =>
    parseCaseFile(path, text),
  );
  return sortCases(assertUniqueIds(cases));
}
