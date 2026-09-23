# Contracts

## Document (shipped)

Pydantic under ``xpict.contracts`` — **strict subset** of future nested
``PictSpec``:

| Model | Role |
| --- | --- |
| ``DepictSpec`` / ``MolNode`` | Declarative document: ``type: "mol"`` or ``type: "group"`` + ``children`` |
| ``Scene`` (+ primitives) | Paint ABI (Rust / JS / Python serializers) |

``MoleculeLayout`` remains **Python-internal** for the legacy ``Pict`` draw
stack only — not a shipped contract. The old ``LayoutResult`` wrapper and
``schema/layout.schema.json`` are removed.

Live **EdgePlan / EdgeResult** and **Scene** (paint ABI) are defined in Rust
(``xpict-core``). TypeScript is generated (``make types`` → ``js/src/generated``);
JSON Schema is schemars. See [`docs/dev/typebridge.md`](dev/typebridge.md).

Every document must also validate as ``xpict.future.PictSpec``.

Committed JSON Schema:

- ``schema/xpict.schema.json`` — ``DepictSpec`` (Pydantic today)
- ``schema/scene.schema.json`` — scene graph from **Rust** (schemars)
- ``schema/edge-plan.schema.json`` / ``edge-result.schema.json`` — from **Rust**
  (schemars)

## Single-molecule client

``mol`` / ``render`` / ``toSvg`` (JS, Python, Rust) — imperative one-mol API. Options:
``color``, ``atom_shade``, ``bond_shade``, ``star_labels``, ``weight``
(default ``1`` = house; min ``2/3``), ``scale``, ``align_to``, ``atom_map``
(``(query, template)`` pairs; requires ``align_to``; skips MCS), ``id``. The
document path uses this layer internally where it
exists. Prefer the document two-pass (``plan_edge`` /
``process_edge_plan`` / ``render_doc``) for nested ``DepictSpec``.


## Future (design)

Full nested ``PictSpec`` lives in ``xpict.future`` and
``schema/future/xpict.schema.json``. Feedback welcome.

## Runtime vs document

- **Mol node:** ``smiles`` / ``cxsmiles`` / ``molfile``, ``id``, ``color``,
  ``shade``, ``star_labels`` (chem markup; encounter order), ``scale``,
  ``weight``, ``align_to`` (id string or ``{ref, atom_map?, min_atoms?}``).
  Markush / star text also via CXSMILES aliases when ``star_labels``
  is omitted.
- **Group:** ``align`` (bool) — when true, later children align onto the first
  (or each child's ``align_to``).
- **Single-mol ``render`` only:** ``align_to`` as Mol / Rendered (or pose
  molblock), plus flat ``atom_shade`` / ``bond_shade`` / ``atom_map``.
  Document ``rgroups`` is not public yet.
- **Runtime only:** layout backend, output format.

Label scripts: [Label markup](label-markup.md).

## Partial backend support

If a backend cannot honor an option, it must
``warnings.warn(..., PictBackendWarning)`` and continue best-effort.

## Document two-pass (EdgePlan)

Hosts do **not** apply CX / star / shade chrome themselves. Flow:

1. Core ``plan_edge(DepictSpec)`` → ``EdgePlan`` (coord_gen forest)
2. Host ``process_edge_plan`` (RDKit layout / align) → ``EdgeResult``
3. Core ``render_doc(DepictSpec, EdgeResult)`` → painted scenes
   (CX aliases, ``star_labels``, color, shade, scale, weight)

Document mol ``align_to`` is an id string or
``{ "ref": id, "atom_map"?, "min_atoms"? }`` (maps to EdgePlan ``AlignOpts``).
Group ``align: true`` builds the forest with default parent = first child.

Schema: ``schema/edge-plan.schema.json``, ``schema/edge-result.schema.json``.

