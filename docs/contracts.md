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

## Single-molecule client

``mol`` / ``render`` / ``toSvg`` (JS, Rust) — imperative one-mol API. Options:
``color``, ``atom_shade``, ``bond_shade``, ``star_labels``, ``bold_labels``,
``align_to``, ``id``. The document path uses this layer internally where it
exists.

## Future (design)

Full nested ``PictSpec`` lives in ``xpict.future`` and
``schema/future/xpict.schema.json``. Feedback welcome.

## Runtime vs document

- **Mol node:** ``smiles`` / ``cxsmiles`` / ``molfile``, ``id``, ``color``,
  ``shade``. Markush / star text on the document path: CX braced aliases.
- **Single-mol ``render`` only:** ``align_to`` as Mol / Rendered (or pose
  molblock), plus flat ``atom_shade`` / ``star_labels`` (chem markup).
  Document ``rgroups`` is not public yet.
- **Runtime only:** layout backend, output format.

Label scripts: [Label markup](label-markup.md).

## Partial backend support

If a backend cannot honor an option, it must
``warnings.warn(..., PictBackendWarning)`` and continue best-effort.
