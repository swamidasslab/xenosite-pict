# Plan: Rust-first live contracts

**Status:** in progress (Phase 0 spike done → pivot).  
**Original proposal:** [typebridge](https://crates.io/crates/typebridge).  
**Chosen toolchain after spike:** **schemars** (JSON Schema) + **ts-rs** (TypeScript). Python Pydantic for live stays hand-written for now (see below).

## Goal (unchanged)

| Surface | Source of truth | Client bindings |
| --- | --- | --- |
| **Live** (shipped) | Rust (`xpict-core` serde types) | Generated **TypeScript** (ts-rs); committed JSON Schema (schemars); hand Pydantic until a faithful emitter exists |
| **Future** (design) | Python `xpict.future` | Hand / design TS under `js/src/future/` |

Invariant: every live document must still validate as future `PictSpec`.

## LayoutResult — not a live shared ABI

| Type | Role |
| --- | --- |
| `MoleculeLayout` / `AtomLayout` / `BondLayout` | **Python-internal** layout→draw stack (`backends`, `align`, `draw/*`, legacy `api.layout`) |
| `LayoutResult` | Thin `{ molecules: […] }` wrapper around that — almost only `api.layout()` + schema export |
| JS `LayoutResult` in `rdkit-layout.ts` | **Different shape** (single-mol host result); same name only |

**Not in `xpict-core`.** Live cross-language wire types are `MoleculeIn` / `EdgePlan` / `EdgeResult`. Do **not** feed layout into Rust-first codegen. Treat as Python-only (or delete `schema/layout.schema.json` from the “shipped contracts” story when flipping schema SoT).

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
    ├─ feature `codegen` → schemars::JsonSchema  → schema/*.json (live)
    └─ feature `codegen` → ts_rs::TS             → js/src/generated/*.ts
python/xpict/contracts/*   hand StrictModel (parity tests vs Rust fixtures)
xpict.future               unchanged Pydantic SoT → schema/future/
```

Commands:

- `make types` — build/export with `--features codegen`, copy TS + write live schemas  
- `make types-check` — regenerate and `git diff --exit-code`

## Phased delivery (revised)

0. ~~Spike typebridge~~ → **done; pivoted**  
1. **Infra + EdgePlan/EdgeResult** — annotate edge + `MoleculeIn` inputs; generate TS + schema; JS imports generated types; Python hand + fixture parity  
2. **DepictSpec / MolNode / AlignTo** — same pattern  
3. **Scene** paint ABI types  
4. Retire Pydantic as live schema SoT in `export_schema.py` (future-only); drop layout from live contract docs  
5. Revisit Python codegen if/when an emitter matches StrictModel quality (or write a thin custom one)

## What stays hand-written

- Host logic (`process_edge_plan`, SVG serializers, single-mol `Mol` / `Rendered`)  
- Live **Python** contracts until codegen quality catches up  
- Future PictSpec  
- Wasm still stringly JSON FFI; generated TS types the payloads

## Risks

| Risk | Mitigation |
| --- | --- |
| Hand Python drifts from Rust | Fixture round-trip CI (Rust serde ↔ Python model) |
| ts-rs / schemars version churn | Pin versions; `codegen` feature keeps them off crates.io default graph |
| Optional vs null in TS | `#[ts(optional)]` aligned with `skip_serializing_if` |
