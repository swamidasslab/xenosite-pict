/**
 * EdgePlan / EdgeResult — host callback ABI (coord_gen / align).
 * Keep in sync with ``xpict.contracts.edge`` / ``xpict_core::edge``.
 */

import {
  layoutWithRdkit,
  type LayoutMeta,
  type MoleculeIn,
} from "./layout/rdkit-layout.js";

export const MIN_MCS_ATOMS = 3;

export type AlignOpts = {
  atom_map?: Array<[number, number]> | null;
  min_atoms?: number | null;
};

export type MolTemplate = {
  id: string;
  smiles?: string | null;
  cxsmiles?: string | null;
  molfile?: string | null;
  align?: AlignOpts | null;
  template_for?: MolTemplate[];
};

export type CoordGenTask = {
  type: "coord_gen";
  roots: MolTemplate[];
};

export type EdgePlan = {
  version: 1;
  tasks: CoordGenTask[];
};

export type CoordGenMoleculeResult = {
  id: string;
  ok: boolean;
  method: LayoutMeta["method"];
  used_map?: Array<[number, number]> | null;
  molecule?: MoleculeIn | null;
  error?: string | null;
};

export type CoordGenTaskResult = {
  type: "coord_gen";
  ok: boolean;
  molecules: CoordGenMoleculeResult[];
};

export type EdgeResult = {
  version: 1;
  results: CoordGenTaskResult[];
};

function sourceOf(node: MolTemplate): string {
  for (const v of [node.smiles, node.cxsmiles, node.molfile]) {
    if (v != null && String(v).trim()) return String(v);
  }
  throw new Error(`MolTemplate ${node.id}: empty structure`);
}

export function validateEdgePlan(plan: EdgePlan): EdgePlan {
  if (plan.version !== 1) throw new Error(`unsupported EdgePlan version ${plan.version}`);
  const seen = new Set<string>();
  const walk = (node: MolTemplate, isRoot: boolean) => {
    if (seen.has(node.id)) throw new Error(`duplicate MolTemplate id ${node.id}`);
    seen.add(node.id);
    sourceOf(node);
    if (isRoot && node.align != null) {
      throw new Error(`MolTemplate ${node.id}: roots must have align=null`);
    }
    for (const c of node.template_for ?? []) walk(c, false);
  };
  for (const task of plan.tasks) {
    for (const root of task.roots) walk(root, true);
  }
  return plan;
}

export function buildAlignPlan(opts: {
  templateSource: string;
  querySource: string;
  atomMap?: Array<[number, number]> | null;
  templateId?: string;
  queryId?: string;
}): EdgePlan {
  return {
    version: 1,
    tasks: [
      {
        type: "coord_gen",
        roots: [
          {
            id: opts.templateId ?? "m_0",
            smiles: opts.templateSource,
            align: null,
            template_for: [
              {
                id: opts.queryId ?? "m_1",
                smiles: opts.querySource,
                align: { atom_map: opts.atomMap ?? null },
                template_for: [],
              },
            ],
          },
        ],
      },
    ],
  };
}

/** Run all coord_gen tasks; always return flat molecule rows.
 *
 * Align failure → automatic free (unaligned) layout; `method: "none"`,
 * `ok: true` when coords were produced.
 */
export async function processEdgePlan(plan: EdgePlan): Promise<EdgeResult> {
  return (await processEdgePlanWithFrames(plan)).result;
}

/** Like [`processEdgePlan`], plus pose molblocks keyed by mol id. */
export async function processEdgePlanWithFrames(
  plan: EdgePlan
): Promise<{ result: EdgeResult; frames: Map<string, string> }> {
  validateEdgePlan(plan);
  const results: CoordGenTaskResult[] = [];
  const allFrames = new Map<string, string>();

  for (const task of plan.tasks) {
    const rows: CoordGenMoleculeResult[] = [];
    const poses = new Map<string, string>();

    const visit = async (node: MolTemplate, parentId: string | null) => {
      const source = sourceOf(node);
      const atomMap = node.align?.atom_map ?? null;

      const freeLayout = async () =>
        layoutWithRdkit(source, { id: node.id, template: null });

      try {
        if (parentId == null) {
          const laid = await freeLayout();
          rows.push({
            id: node.id,
            ok: true,
            method: "free",
            molecule: laid.molecule,
          });
          poses.set(node.id, laid.molblock);
        } else {
          const template = poses.get(parentId);
          if (!template) throw new Error(`missing parent pose ${parentId}`);
          const laid = await layoutWithRdkit(source, {
            id: node.id,
            template,
            atomMap,
            minAtoms: node.align?.min_atoms ?? MIN_MCS_ATOMS,
          });
          if (laid.meta.method === "atom_map" || laid.meta.method === "mcs") {
            rows.push({
              id: node.id,
              ok: true,
              method: laid.meta.method,
              used_map: laid.meta.used_map ?? null,
              molecule: laid.molecule,
            });
            poses.set(node.id, laid.molblock);
          } else {
            const free = await freeLayout();
            rows.push({
              id: node.id,
              ok: true,
              method: "none",
              molecule: free.molecule,
              error: "align failed; fell back to unaligned coord gen",
            });
            poses.set(node.id, free.molblock);
          }
        }
      } catch (e) {
        try {
          const free = await freeLayout();
          rows.push({
            id: node.id,
            ok: true,
            method: parentId != null ? "none" : "free",
            molecule: free.molecule,
            error:
              parentId != null
                ? `align/layout error (${e instanceof Error ? e.message : String(e)}); fell back to unaligned coord gen`
                : null,
          });
          poses.set(node.id, free.molblock);
        } catch (e2) {
          rows.push({
            id: node.id,
            ok: false,
            method: "none",
            molecule: null,
            error: e2 instanceof Error ? e2.message : String(e2),
          });
          return;
        }
      }
      for (const child of node.template_for ?? []) {
        await visit(child, node.id);
      }
    };

    for (const root of task.roots) {
      await visit(root, null);
    }
    for (const [k, v] of poses) allFrames.set(k, v);
    results.push({
      type: "coord_gen",
      ok: rows.every((r) => r.ok),
      molecules: rows,
    });
  }

  return { result: { version: 1, results }, frames: allFrames };
}
