/**
 * GitHub Pages demo — two SMILES, query aligned to template, editable options.
 * Redraws on any input change (no Draw button).
 */
import { xpict } from "./pkg/index.js";

const $ = (id) => document.getElementById(id);

const form = $("form");
const statusEl = $("status");
const out1 = $("out1");
const out2 = $("out2");

const DEBOUNCE_MS = 180;
let debounceTimer = 0;
let drawGen = 0;

function setStatus(text, isError = false) {
  statusEl.textContent = text;
  statusEl.classList.toggle("error", isError);
}

function parseFloatList(raw) {
  const text = raw.trim();
  if (!text) return undefined;
  const vals = text.split(/[\s,]+/).filter(Boolean).map(Number);
  if (vals.some((n) => Number.isNaN(n))) {
    throw new Error("shade values must be numbers");
  }
  return vals;
}

function parseIndexList(raw) {
  const text = raw.trim();
  if (!text) return undefined;
  const vals = text.split(/[\s,]+/).filter(Boolean).map((s) => Number(s));
  if (vals.some((n) => !Number.isInteger(n) || n < 0)) {
    throw new Error("mark_atoms must be non-negative integers");
  }
  return vals;
}

function parseBondPairs(raw) {
  const text = raw.trim();
  if (!text) return undefined;
  const pairs = text.split(/[\s,]+/).filter(Boolean).map((tok) => {
    const m = tok.match(/^(\d+)[-:](\d+)$/);
    if (!m) throw new Error(`bad bond mark "${tok}" (use begin-end)`);
    return [Number(m[1]), Number(m[2])];
  });
  return pairs;
}

function readOptions() {
  return {
    align: $("align").checked,
    color1: $("color1").value,
    color2: $("color2").value,
    mark_atoms: parseIndexList($("mark_atoms").value),
    mark_bonds: parseBondPairs($("mark_bonds").value),
    atom_shade: parseFloatList($("atom_shade").value),
    bond_shade: parseFloatList($("bond_shade").value),
  };
}

function showSvg(el, svg) {
  el.classList.remove("empty");
  el.innerHTML = svg;
}

function clearOut() {
  out1.classList.add("empty");
  out2.classList.add("empty");
  out1.innerHTML = "";
  out2.innerHTML = "";
}

async function draw() {
  const gen = ++drawGen;
  const smiles1 = $("smiles1").value.trim();
  const smiles2 = $("smiles2").value.trim();
  if (!smiles1 || !smiles2) {
    clearOut();
    setStatus("Both SMILES are required.", true);
    return;
  }

  let opts;
  try {
    opts = readOptions();
  } catch (err) {
    setStatus(err.message || String(err), true);
    return;
  }

  setStatus("Loading RDKit + wasm…");

  try {
    const template = xpict.mol(smiles1);
    const rendered1 = await xpict.render(template, {
      color: opts.color1,
    });
    if (gen !== drawGen) return;
    showSvg(out1, xpict.toSvg(rendered1.scene));

    const queryOpts = {
      color: opts.color2,
      ...(opts.mark_atoms ? { mark_atoms: opts.mark_atoms } : {}),
      ...(opts.mark_bonds ? { mark_bonds: opts.mark_bonds } : {}),
      ...(opts.atom_shade ? { atom_shade: opts.atom_shade } : {}),
      ...(opts.bond_shade ? { bond_shade: opts.bond_shade } : {}),
      ...(opts.align ? { align_to: template } : {}),
    };
    const rendered2 = await xpict.render(xpict.mol(smiles2), queryOpts);
    if (gen !== drawGen) return;
    showSvg(out2, xpict.toSvg(rendered2.scene));

    setStatus(
      `OK — template ${rendered1.coords.length} atoms · query ${rendered2.coords.length} atoms` +
        (opts.align ? " · aligned" : " · free layout")
    );
  } catch (err) {
    if (gen !== drawGen) return;
    console.error(err);
    setStatus(`FAIL: ${err?.message || err}`, true);
  }
}

function scheduleDraw() {
  window.clearTimeout(debounceTimer);
  debounceTimer = window.setTimeout(() => {
    void draw();
  }, DEBOUNCE_MS);
}

form.addEventListener("submit", (ev) => {
  ev.preventDefault();
  void draw();
});

form.addEventListener("input", scheduleDraw);
form.addEventListener("change", scheduleDraw);

$("swap").addEventListener("click", () => {
  const a = $("smiles1").value;
  $("smiles1").value = $("smiles2").value;
  $("smiles2").value = a;
  scheduleDraw();
});

clearOut();
void draw();
