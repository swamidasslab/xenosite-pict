# Contracts

## Live (shipped)

Pydantic under ``xpict.contracts`` — **strict subset** of future nested
``PictSpec``:

| Model | Role |
| --- | --- |
| ``DepictSpec`` / ``MolNode`` | Preferred document: ``type: "mol"`` or ``type: "group"`` + ``children`` |
| ``Scene`` (+ primitives) | Paint ABI (Rust / JS / Python serializers) |
| ``MoleculeLayout`` / ``LayoutResult`` | Backend layout result |

Every live document must also validate as ``xpict.future.PictSpec``.

JSON Schema (committed):

- ``schema/xpict.schema.json`` — **live** ``DepictSpec``
- ``schema/scene.schema.json`` — scene graph
- ``schema/layout.schema.json`` — layout result

```bash
uv run xpict-export-schema
```

## Simple client (not a JSON document)

``mol`` / ``render`` / ``toSvg`` — imperative single-mol API. Options:
``color``, ``atom_shade``, ``bond_shade``, ``star_labels``, ``bold_labels``,
``align_to``, ``id``. Document path calls this layer internally.

## Future (design / refinement)

Full nested ``PictSpec`` lives in ``xpict.future`` and
``schema/future/xpict.schema.json``. Comments welcome.

## Runtime vs document

- **Live mol node:** ``smiles`` / ``cxsmiles`` / ``molfile``, ``id``, ``color``,
  ``shade``. Markush / star text on the document path: CX braced aliases.
- **Simple ``render`` only:** ``align_to`` as Mol / Rendered (or pose molblock),
  plus flat ``atom_shade`` / ``star_labels`` (chem markup for scripts).
  Document ``rgroups`` is not public yet (future PictSpec).
- **Runtime only:** layout backend, output format.

Label scripts: [Label markup](label-markup.md).

## Partial backend support

If a backend cannot honor an option, it must ``warnings.warn(..., PictBackendWarning)`` and continue best-effort.
