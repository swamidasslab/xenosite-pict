# Plan: Rust-first live contracts

**Status:** live EdgePlan + Scene + DepictSpec cut over for TS, JSON Schema, and Python.  
**Original proposal:** [typebridge](https://crates.io/crates/typebridge).  
**Chosen toolchain after spike:** **schemars** (JSON Schema) + **ts-rs** (TypeScript) + custom schemars→StrictModel Python emitter.

## Goal (unchanged)

| Surface | Source of truth | Client bindings |
| --- | --- | --- |
| **Live** (shipped) | Rust (`xpict-core` serde types) | Generated **TypeScript** (ts-rs); committed JSON Schema (schemars); generated **Pydantic** (`scripts/generate_live_python.py`) |
| **Future** (design) | Python `xpict.future` | `schema/future/`; no parallel JS tree |

Invariant: every live document must still validate as future `PictSpec`.

## LayoutResult — removed (defunct)

The ``LayoutResult`` wrapper and ``schema/layout.schema.json`` are **gone**.
They were never a live cross-language ABI.

| Still exists | Role |
| --- | --- |
| ``MoleculeLayout`` / ``AtomLayout`` / ``BondLayout`` | **Python-internal** only — legacy ``Pict`` → backends → ``draw/*`` until that stack is retired |
| JS type formerly named ``LayoutResult`` in ``rdkit-layout.ts`` | Renamed ``HostLayoutResult`` (unrelated single-mol host result) |

Live wire types remain ``MoleculeIn`` / ``EdgePlan`` / ``EdgeResult``.

## Phase 0 spike — typebridge rejected

Annotated AlignTo / DepictSpec / EdgeTask / EdgePlan with `TypeWriter`. Output was **wire-incompatible** with serde:

| Case | serde JSON (truth) | typebridge emitted |
| --- | --- | --- |
| `#[serde(tag = "type", rename_all = "lowercase")]` DepictSpec | `"type": "mol"` / `"group"` | `"Mol"` / `"Group"` |
| `rename_all = "snake_case"` EdgeTask | `"coord_gen"` | `"CoordGen"` |
| `#[serde(untagged)]` AlignTo | `string \| {ref,…}` | Python: `Ref { value_0 }` / `Spec { value_0 }` wrappers (TS was OK) |
| Pydantic | v2 desired | v1-style `class Config` despite `pydantic_v2 = true` |

**Do not use typebridge** until those bugs are fixed upstream.

### Alternatives tried

| Tool | Result |
| --- | --- |
| **schemars 0.8** | Correct tags (`mol` / `coord_gen`), correct untagged AlignTo, usable `$defs` |
| **ts-rs 10** | Correct tags + AlignTo; use `#[ts(optional)]` for `Option` + `skip_serializing_if` |
| **datamodel-codegen** from schemars | Wire-correct but awkward (`RootModel`, `Type`/`Type1` enums) — not good enough to replace hand `StrictModel` yet |

## Chosen pipeline

```
xpict-core (serde types)
    ├─ feature `codegen` → schemars::JsonSchema  → schema/*.json (live: edge, scene, xpict)
    ├─ feature `codegen` → ts_rs::TS             → js/src/generated/*.ts
    └─ schemars JSON → scripts/generate_live_python.py → python/xpict/contracts/{edge,scene,depict}.py
       (schema-driven only: MODULES maps files→output; titles become roots;
        kind→*Prim, type+FooBar→{Val}Bar, Spec→*Node, reuse matching defs;
        RootModel for tagged roots)
python/xpict/contracts/__init__.py  hand: DepictSpec.mols() + MolSpec alias
python/xpict/contracts/layout.py    hand (Python-internal MoleculeLayout only)
xpict.future                        unchanged Pydantic SoT → schema/future/
```

Commands:

- `make types` — TS + live schemas + live Python contracts  
- `make types-check` — regenerate and `git diff --exit-code`

## Phased delivery (revised)

0. ~~Spike typebridge~~ → **done; pivoted**  
1. ~~**Infra + EdgePlan/EdgeResult**~~ — annotate edge + `MoleculeIn` inputs; generate TS + schema; JS imports generated types  
2. ~~**Scene** paint ABI~~ — generated TS + schema; JS `scene-svg` on `scene-abi`  
3. ~~**DepictSpec / MolNode / AlignTo**~~ — generated TS + schema; JS `depict-abi`  
4. ~~**Python live contracts**~~ — `scripts/generate_live_python.py` from schemars (`edge`/`scene`/`depict`); `layout.py` stays hand (internal)  
5. Revisit emitter polish (validators, nicer field order) as needed  

## What stays hand-written

- Host logic (`process_edge_plan`, SVG serializers, single-mol `Mol` / `Rendered`)  
- `python/xpict/contracts/__init__.py` — `DepictSpec.mols()` + `MolSpec` alias  
- `python/xpict/contracts/layout.py` (Python-internal MoleculeLayout)  
- Future PictSpec (Python + `schema/future/` only — no JS mirror)  
- Wasm still stringly JSON FFI; generated TS/Python type the payloads  
- JS `GroupNode.align?` remapped optional (ts-rs cannot optional-ize non-`Option` serde defaults)

## Risks

| Risk | Mitigation |
| --- | --- |
| Hand Python drifts from Rust | Fixture round-trip CI (Rust serde ↔ Python model) |
| ts-rs / schemars version churn | Pin versions; `codegen` feature keeps them off crates.io default graph |
| Optional vs null in TS | `#[ts(optional)]` aligned with `skip_serializing_if` |
