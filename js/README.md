# JS/TS processor

Browser/Node implementation of the same JSON contracts as Python (`../schema`).

- **Types:** `src/types.ts` mirrors PictSpec (schemas remain source of truth)
- **Mol layout:** Indigo WASM (preferred); RDKit MinimalLib optional/limited
- **Diagram layout:** [elkjs](https://github.com/kieler/elkjs)
- **Draw:** shared Scene → SVG rules (parity with Python)

`render()` currently validates PictSpec shape only.
