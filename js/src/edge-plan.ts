/**
 * EdgePlan / EdgeResult — host callback ABI (coord_gen / align).
 *
 * Types are generated from ``xpict-core`` (``make types`` / ts-rs).
 * Keep runtime helpers here.
 */

import { layoutWithRdkit } from "./layout/rdkit-layout.js";
import { validateEdgePlanJson } from "./native.js";
import type {
  AlignOpts,
  CoordGenMoleculeResult,
  EdgePlan,
  EdgeResult,
  EdgeTask,
  EdgeTaskResult,
  MolTemplate,
} from "./generated/edge.js";

export type {
  AlignOpts,
  AtomIn,
  BondIn,
  CoordGenMoleculeResult,
  CoordMethod,
  EdgePlan,
  EdgeResult,
  EdgeTask,
  EdgeTaskResult,
  MoleculeIn as EdgeMoleculeIn,
  MolTemplate,
} from "./generated/edge.js";

/** @deprecated Prefer {@link EdgeTask} (only variant today). */
export type CoordGenTask = Extract<EdgeTask, { type: "coord_gen" }>;
/** @deprecated Prefer {@link EdgeTaskResult}. */
export type CoordGenTaskResult = Extract<
  EdgeTaskResult,
  { type: "coord_gen" }
>;

export const MIN_MCS_ATOMS = 3;

/** Host layout MoleculeIn → wire ABI (charge always present in Rust). */
function asWireMolecule(
  mol: import("./layout/rdkit-layout.js").MoleculeIn
): NonNullable<CoordGenMoleculeResult["molecule"]> {
  return {
    ...mol,
    atoms: mol.atoms.map((a) => ({ ...a, charge: a.charge ?? 0 })),
  } as NonNullable<CoordGenMoleculeResult["molecule"]>;
}

function sourceOf(node: MolTemplate): string {
  for (const v of [node.smiles, node.cxsmiles, node.molfile]) {
    if (v != null && String(v).trim()) return String(v);
  }
  throw new Error(`MolTemplate ${node.id}: empty structure`);
}

/** Structural checks via Rust core (wasm must be initialized). */
export function validateEdgePlan(plan: EdgePlan): EdgePlan {
  return JSON.parse(validateEdgePlanJson(JSON.stringify(plan))) as EdgePlan;
}

export function buildAlignPlan(opts: {
  templateSource: string;
  querySource: string;
  atomMap?: Array<[number, number]> | null;
  templateId?: string;
  queryId?: string;
}): EdgePlan {
  const childAlign: AlignOpts | undefined =
    opts.atomMap != null ? { atom_map: opts.atomMap } : undefined;
  return {
    version: 1,
    tasks: [
      {
        type: "coord_gen",
        roots: [
          {
            id: opts.templateId ?? "m_0",
            smiles: opts.templateSource,
            template_for: [
              {
                id: opts.queryId ?? "m_1",
                smiles: opts.querySource,
                ...(childAlign ? { align: childAlign } : {}),
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
            molecule: asWireMolecule(laid.molecule),
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
              used_map: laid.meta.used_map ?? undefined,
              molecule: asWireMolecule(laid.molecule),
            });
            poses.set(node.id, laid.molblock);
          } else {
            const free = await freeLayout();
            rows.push({
              id: node.id,
              ok: true,
              method: "none",
              molecule: asWireMolecule(free.molecule),
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
            molecule: asWireMolecule(free.molecule),
            ...(parentId != null
              ? {
                  error: `align/layout error (${e instanceof Error ? e.message : String(e)}); fell back to unaligned coord gen`,
                }
              : {}),
          });
          poses.set(node.id, free.molblock);
        } catch (e2) {
          rows.push({
            id: node.id,
            ok: false,
            method: "none",
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
