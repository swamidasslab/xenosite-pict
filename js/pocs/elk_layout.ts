#!/usr/bin/env node
/**
 * POC: read ELK JSON graph from stdin, layout with elkjs, write laid-out graph to stdout.
 * Usage: npx tsx pocs/elk_layout.ts < graph.json > laid.json
 */
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

type ElkLayout = {
  layout(graph: unknown): Promise<unknown>;
};

type ElkModule = {
  default?: new () => ElkLayout;
} & (new () => ElkLayout);

const __dirname = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

function loadElk(): ElkModule {
  try {
    return require("elkjs") as ElkModule;
  } catch {
    return require(join(__dirname, "..", "node_modules", "elkjs")) as ElkModule;
  }
}

const chunks: Buffer[] = [];
for await (const c of process.stdin) {
  chunks.push(Buffer.isBuffer(c) ? c : Buffer.from(c));
}
const graph = JSON.parse(Buffer.concat(chunks).toString("utf8") || "{}") as unknown;

const ELK = loadElk();
const ElkCtor = ELK.default ?? ELK;
const elk = new ElkCtor();
const laid = await elk.layout(graph);
process.stdout.write(JSON.stringify(laid));
