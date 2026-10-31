import type { DepictSpec } from "../../src/api.js";

/** Sidebar sections — one behavior family each. Folder name under `cases/`. */
export type ReviewGroup =
  | "structure"
  | "bonds-stereo"
  | "hetero"
  | "aromatic"
  | "ink"
  | "shade"
  | "markush"
  | "chrome"
  | "region"
  | "align"
  | "scheme"
  | "compose";

export const GROUP_TITLE: Record<ReviewGroup, string> = {
  structure: "Structure",
  "bonds-stereo": "Bonds & stereo",
  hetero: "Heteroatoms, charge, radicals",
  aromatic: "Aromatic paint",
  ink: "Ink (color, weight, scale)",
  shade: "Shade",
  markush: "Markush & markup",
  chrome: "Chrome",
  region: "Region annotation",
  align: "Align",
  scheme: "Schemes",
  compose: "Compose (group / grid)",
};

export const GROUP_ORDER: ReviewGroup[] = [
  "structure",
  "bonds-stereo",
  "hetero",
  "aromatic",
  "ink",
  "shade",
  "markush",
  "chrome",
  "region",
  "align",
  "scheme",
  "compose",
];

export type ReviewCase = {
  /** Stable snapshot / notes key (filename stem; unique across groups). */
  id: string;
  group: ReviewGroup;
  title: string;
  /** What this case is meant to show when reviewing. */
  expect: string;
  spec: DepictSpec;
  /** Repo-relative path, e.g. `cases/structure/ethanol.yaml`. */
  file: string;
  /** File text (YAML/JSON) so the UI can show the source. */
  raw: string;
};
