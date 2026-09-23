# Future PictSpec (design / refinement)

Full nested declarative document — groups, reactions, annotations, shorthand,
legacy `{ molecules, diagram }` lift.

The **live preferred** contract is `xpict.contracts.depict` — a **strict
subset** of this tree (`type: "mol"` | `type: "group"` + `children`, plus
`shade` / …). Document `rgroups` and richer nodes stay here until they
graduate. Every live document must validate here too.

JSON Schema: `schema/future/xpict.schema.json`.

## Feedback welcome

Open a GitHub issue linking this directory / schema, or a PR that only touches
`xpict.future` / `schema/future/`.

Promote fields into `xpict.contracts` as they gain paint support.
