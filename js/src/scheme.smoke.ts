/**
 * Reaction-scheme smoke: ``xpict.depict({ type: "reaction_scheme", … })``.
 * Same product path as a lone mol — Rust render packs cells (ELK + arrows).
 * Run: `npx tsx src/scheme.smoke.ts`
 */
import { xpict } from "./api.js";
import type {
  DepictSpec,
  EdgeArrow,
  EdgeNode,
  EdgeRouting,
  LayoutAlgorithm,
  LayoutOpts,
  MolNode,
  Primitive,
  Scene,
  TextNode,
} from "./api.js";

if (typeof document !== "undefined") {
  throw new Error("scheme smoke expects Node (server-side) without document");
}

type SchemeOpts = {
  a?: string;
  b?: string;
  label?: string | null;
  arrow?: EdgeArrow;
  dashed?: boolean;
  color?: string;
  edge_routing?: EdgeRouting;
  algorithm?: LayoutAlgorithm;
  node_spacing?: number;
  layer_spacing?: number;
};

function scheme(opts: SchemeOpts = {}): DepictSpec {
  const a = opts.a ?? "CCO";
  const b = opts.b ?? "CC=O";
  const label = opts.label === undefined ? "ADH" : opts.label;
  const children: Array<MolNode | TextNode | EdgeNode> = [
    { type: "mol", id: "A", smiles: a },
    { type: "mol", id: "B", smiles: b },
  ];
  const edge: EdgeNode = {
    type: "edge",
    sources: "A",
    targets: "B",
    arrow: opts.arrow ?? "forward",
    dashed: opts.dashed ?? false,
  };
  if (opts.color !== undefined) edge.color = opts.color;
  if (opts.edge_routing !== undefined) edge.edge_routing = opts.edge_routing;
  if (label) {
    children.splice(1, 0, { type: "text", id: "lab", text: label });
    edge.label = { id: "lab", pos: "above" };
  }
  children.push(edge);
  const layout: LayoutOpts = {};
  if (opts.algorithm !== undefined) layout.algorithm = opts.algorithm;
  if (opts.edge_routing !== undefined) layout.edge_routing = opts.edge_routing;
  if (opts.node_spacing !== undefined) layout.node_spacing = opts.node_spacing;
  if (opts.layer_spacing !== undefined) layout.layer_spacing = opts.layer_spacing;
  if (Object.keys(layout).length) {
    return { type: "reaction_scheme", children, layout };
  }
  return { type: "reaction_scheme", children };
}

function primClass(p: Primitive): string {
  return p.cls ?? "";
}

function overlayClasses(scene: Scene): string[] {
  return scene.overlays.map(primClass).filter(Boolean);
}

{
  const rows = await xpict.depict(scheme());
  if (rows.length !== 1) throw new Error(`scheme cells ${rows.length}`);
  const scene = rows[0]!.scene;
  if (scene.viewports.length !== 2) {
    throw new Error(`scheme viewports ${scene.viewports.length}`);
  }
  const classes = overlayClasses(scene);
  if (!classes.some((c) => c.includes("edge"))) {
    throw new Error(`scheme missing edge overlay, got ${JSON.stringify(classes)}`);
  }
  if (!classes.some((c) => c.includes("label"))) {
    throw new Error(`scheme missing label overlay, got ${JSON.stringify(classes)}`);
  }
  const svg = xpict.toSvg(scene);
  if (!svg.includes("<svg") || !(svg.includes("edge") || svg.includes("M "))) {
    throw new Error("scheme SVG missing arrow/edge");
  }
}

{
  const svg = xpict.toSvg((await xpict.depict(scheme({ dashed: true, color: "#06c" })))[0]!.scene);
  if (!svg.includes("<svg")) throw new Error("dashed scheme SVG");
}

{
  for (const arrow of ["equilibrium", "open", "line"] as const) {
    const scene = (await xpict.depict(scheme({ arrow, label: null })))[0]!.scene;
    if (!scene.overlays.length) throw new Error(`expected overlays for ${arrow}`);
  }
}

{
  const scene = (
    await xpict.depict(
      scheme({ label: null, edge_routing: "orthogonal", algorithm: "layered" })
    )
  )[0]!.scene;
  const paths = scene.overlays.filter((p) => p.kind === "path" && p.d);
  if (!paths.length) throw new Error("orthogonal scheme has no path overlays");
  if (!paths.some((p) => p.kind === "path" && p.d.includes("M"))) {
    throw new Error("orthogonal scheme path missing M");
  }
}

{
  const rows = await xpict.depict({
    type: "reaction_scheme",
    children: [
      { type: "mol", id: "A", smiles: "C" },
      { type: "mol", id: "B", smiles: "N" },
      { type: "mol", id: "C", smiles: "O" },
      {
        type: "edge",
        sources: ["A", "B"],
        targets: ["C"],
        arrow: "forward",
        dashed: false,
      },
    ],
  });
  const scene = rows[0]!.scene;
  if (scene.viewports.length !== 3) {
    throw new Error(`multi-endpoint viewports ${scene.viewports.length}`);
  }
  if (!scene.overlays.length) throw new Error("multi-endpoint missing overlays");
}

{
  const svg = xpict.toSvg(
    (
      await xpict.depict(
        scheme({ label: null, node_spacing: 8, layer_spacing: 4, algorithm: "layered" })
      )
    )[0]!.scene
  );
  if (!svg.includes("<svg")) throw new Error("spacing knobs scheme SVG");
}

{
  const rows = await xpict.depict({
    type: "reaction_scheme",
    children: [
      {
        type: "mol",
        id: "ph",
        smiles: "c1ccccc1O",
        name: "phenol",
        shade: { atoms: [0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0], vmin: 0, vmax: 1 },
        atom_number: "index",
        atom_label: { "2": "ortho", "6": "exo" },
        circle: { atoms: [6], stroke: { color: "#06c", width: 1.4 } },
        region: {
          style: "box",
          atoms: [0, 1, 2, 3, 4, 5],
          exclude: [],
          fill: { color: "#48a", opacity: 0.12 },
          stroke: { color: "#48a", width: 1.0 },
          text: "ring",
        },
        callout: {
          atoms: [6],
          text: "phenol OH",
          arrow: true,
          prefer: "above",
        },
      },
      { type: "mol", id: "et", smiles: "CCO" },
      { type: "edge", sources: "ph", targets: "et", arrow: "forward", dashed: false },
    ],
  });
  const svg = xpict.toSvg(rows[0]!.scene);
  for (const needle of [
    "region",
    "callout",
    "atom-circle",
    "atom-number",
    "atom-label",
    "mol-caption",
    "shade",
  ]) {
    if (!svg.includes(needle)) {
      throw new Error(`scheme mol missing ${needle}`);
    }
  }
}

console.log("scheme smoke ok");
