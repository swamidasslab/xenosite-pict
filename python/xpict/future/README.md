# Future PictSpec (not shipped)

This package holds the **full nested declarative document** models that are
still being designed:

- Nested nodes (`group` / `grid` / `stack` / `reaction` / `network`)
- Molecule chrome beyond the MVP (`annotations`, `rtable`, `ring_attachments`, …)
- Label / R-group shorthand expanders
- Legacy flat `{ molecules, diagram }` lift/flatten

The **live preferred** document contract is `xpict.contracts.depict`
(`MolSpec` / `DepictSpec`) — the subset painted today. The simple
`mol` / `render` / `to_svg` client is for single-mol callers; documents use
that layer internally.

JSON Schema for this tree is exported under `schema/future/xpict.schema.json`.

## Feedback welcome

Comments on this design are encouraged:

- Open a [GitHub issue](https://github.com/swamidasslab/xenosite-pict/issues)
  and link this directory or the schema file, or
- Discuss in a PR that only touches `xpict.future` / `schema/future/`.

Promote fields into `xpict.contracts` as they gain paint support — do not treat
this tree as the published PyPI/npm/crates API yet.
