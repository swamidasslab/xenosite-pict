# Layout notes

## Backend preference

1. **Indigo** — preferred publication-quality 2D coords (also has WASM for `js/`).
2. **RDKit** — strong default; good CXSMILES support.
3. **Open Babel / pybel** — broad format coverage; solid 2D fallback.
4. **Chematic** — last resort; coordinate quality is weaker.
5. **native** — toy stub for tests without chem engines.

Own pure-Python layout engine is deferred: Indigo’s layout alone is large (~17k LOC C++). Wrap first.

## ELK for multi-mol diagrams

Metabolic networks, reaction schemes, and grids use **ELK** for viewport placement (not chemical MCS alignment).

- Python: ship ELK JAR + drive via V8 (scaffold: stub; grid/row fallback active). Packaging hook: `vendor/elk/` (`README.md`, `fetch_elk.sh`); optional `elk.jar` is not committed by default.
- Web: elkjs.

Chemical MCS alignment (xenopict-style) is a separate optional pass before diagram layout.
