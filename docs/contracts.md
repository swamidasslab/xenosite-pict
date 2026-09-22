# Contracts

## Live (shipped)

Pydantic under ``xpict.contracts`` — what the public package implements today:

| Model | Role |
| --- | --- |
| ``MolSpec`` / ``DepictSpec`` | **Preferred** declarative document: mol list → ``Rendered[]`` |
| ``Scene`` (+ primitives) | Paint ABI (Rust / JS / Python serializers) |
| ``MoleculeLayout`` / ``LayoutResult`` | Backend layout result |

JSON Schema (committed):

- ``schema/xpict.schema.json`` — **live** ``DepictSpec``
- ``schema/scene.schema.json`` — scene graph
- ``schema/layout.schema.json`` — layout result

```bash
uv run xpict-export-schema
```

Language clients also expose a **simple** single-mol path
(``mol`` / ``render`` / ``toSvg``) that the document path uses internally.

## Future (design / refinement)

Full nested ``PictSpec`` (groups, reactions, annotations, shorthand, diagram
chrome) lives in ``xpict.future`` and ``schema/future/xpict.schema.json``.
**Comments and design PRs on that tree are welcome** — open a GitHub issue and
link those paths.

Import as ``from xpict.future import PictSpec, MoleculeSpec, …``. Lab code
(``Pict``, POCs, nested-schema tests) still uses these models; they are **not**
the publish surface until features land in ``contracts``.

See ``python/xpict/future/README.md`` and ``CONTRIBUTING.md``.

## Runtime vs document

- **In a live ``MolSpec``:** structure string, marks, shade, color, star labels,
  bold labels, ``id``. (No list-index ``align_to``.)
- **Simple ``render`` only:** ``align_to`` as Mol / Rendered (or pose molblock).
- **Runtime only:** layout backend (RDKit / Indigo / native), output format where applicable.

## Partial backend support

If a backend cannot honor an option, it must ``warnings.warn(..., PictBackendWarning)`` and continue best-effort.
