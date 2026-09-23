# Contracts

## Document (shipped)

Pydantic under ``xpict.contracts`` — **strict subset** of future nested
``PictSpec``:

| Model | Role |
| --- | --- |
| ``DepictSpec`` / ``MolNode`` | Declarative document: ``type: "mol"`` or ``type: "group"`` + ``children`` |
| ``Scene`` (+ primitives) | Paint ABI (Rust / JS / Python serializers) |
| ``MoleculeLayout`` / ``LayoutResult`` | Backend layout result |

Every document must also validate as ``xpict.future.PictSpec``.

Committed JSON Schema:

- ``schema/xpict.schema.json`` — ``DepictSpec``
- ``schema/scene.schema.json`` — scene graph
- ``schema/layout.schema.json`` — layout result
- ``schema/edge-plan.schema.json`` / ``edge-result.schema.json`` — host
  ``EdgePlan`` / ``EdgeResult`` (``coord_gen`` forest + flat molecule rows)

## Single-molecule client

``mol`` / ``render`` / ``toSvg`` (JS, Python, Rust) — imperative one-mol API. Options:
``color``, ``atom_shade``, ``bond_shade``, ``star_labels``, ``weight``
(default ``1`` = house; min ``2/3``), ``scale``, ``align_to``, ``atom_map``
(``(query, template)`` pairs; requires ``align_to``; skips MCS), ``id``. The
document path uses this layer internally where it
exists.

## Future (design)

Full nested ``PictSpec`` lives in ``xpict.future`` and
``schema/future/xpict.schema.json``. Feedback welcome.

## Runtime vs document

- **Mol node:** ``smiles`` / ``cxsmiles`` / ``molfile``, ``id``, ``color``,
  ``shade``, ``star_labels`` (chem markup; encounter order), ``scale``,
  ``weight``. Markush / star text also via CXSMILES aliases when ``star_labels``
  is omitted.
- **Single-mol ``render`` only:** ``align_to`` as Mol / Rendered (or pose
  molblock), plus flat ``atom_shade`` / ``bond_shade``.
  Document ``rgroups`` is not public yet.
- **Runtime only:** layout backend, output format.

Label scripts: [Label markup](label-markup.md).

## Partial backend support

If a backend cannot honor an option, it must
``warnings.warn(..., PictBackendWarning)`` and continue best-effort.
