#!/usr/bin/env node
/**
 * POC: read ELK JSON graph from stdin, layout with elkjs, write laid-out graph to stdout.
 * Usage: node elk_layout.mjs < graph.json > laid.json
 */
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

async function loadElk() {
  try {
    return require("elkjs");
  } catch {
    // try local js/node_modules
    return require(join(__dirname, "..", "node_modules", "elkjs"));
  }
}

const chunks = [];
for await (const c of process.stdin) chunks.push(c);
const graph = JSON.parse(Buffer.concat(chunks).toString("utf8") || "{}");

const ELK = await loadElk();
const ElkCtor = ELK.default || ELK;
const elk = new ElkCtor();
const laid = await elk.layout(graph);
process.stdout.write(JSON.stringify(laid));
