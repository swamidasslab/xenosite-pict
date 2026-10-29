/** `<Pict>` SSR: a prerendered svg prop is emitted inline; no prop → fallback. */
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { xpict } from "./index.js";
import { Pict } from "./react.js";

const svg = await xpict.svg("CCO");
const html = renderToString(createElement(Pict, { smiles: "CCO", svg }));
if (!html.includes("<svg") || !html.includes('class="xpict-pict"')) throw new Error(html);
const lazy = renderToString(createElement(Pict, { smiles: "CCO", fallback: "…" }));
if (lazy.includes("<svg") || !lazy.includes("…")) throw new Error(lazy);
console.log("react smoke ok");
