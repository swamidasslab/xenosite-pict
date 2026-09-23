# Plan: Rust-first live contracts via typebridge

**Status:** design — not implemented.  
**Tool:** [`typebridge`](https://crates.io/crates/typebridge) / [`typebridge-cli`](https://crates.io/crates/typebridge-cli) (typewriter SDK; CLI name `typebridge`).

## Goal

| Surface | Source of truth | Client bindings |
| --- | --- | --- |
| **Live** (shipped) | Rust (`xpict-core` serde types) | Generated **TypeScript** + **Pydantic v2** via typebridge |
| **Future** (design) | Python `xpict.future` Pydantic | Hand / design TS under `js/src/future/` (unchanged) |

Live JSON Schema under `schema/*.json` also moves to Rust-first (typebridge `json_schema` emitter or schemars-equivalent from the same annotated types). Future schema stays `PictSpec.model_json_schema()` → `schema/future/`.

Invariant today that we keep: **every live document must still validate as future `PictSpec`**. Graduation = promote future fields into Rust live types, regenerate, shrink the future-only surface.

## Why

Today three hand-synced copies drift:

1. Rust — `crates/xpict-core` (`doc.rs`, `edge.rs`, `scene.rs`) — already the runtime ABI for `plan_edge` / `render_doc` / paint  
2. Python — `xpict.contracts.*` — also SoT for `make schema` / `export_schema.py`  
3. JS — hand types in `api.ts`, `edge-plan.ts`, `draw/scene-svg.ts`, …

Core already deserializes live JSON; clients should not invent a second shape.

## Current map (live)

| Family | Rust | Python | JS | Schema today |
| --- | --- | --- | --- | --- |
| DepictSpec / MolNode / AlignTo | `doc.rs` | `contracts/depict.py` | `api.ts` | `schema/xpict.schema.json` ← Pydantic |
| EdgePlan / EdgeResult | `edge.rs` | `contracts/edge.py` | `edge-plan.ts` | `schema/edge-*.json` ← Pydantic |
| Scene / MoleculeIn | `scene.rs` | `contracts/scene.py` (+ MoleculeIn mostly dict) | `scene-svg.ts`, `rdkit-layout.ts` | `schema/scene.schema.json` ← Pydantic |
| LayoutResult | **none in core** | `contracts/layout.py` | different host shape | `schema/layout.schema.json` ← Pydantic only |

ShadeSpec is awkward: live Python **imports** `ShadeSpec` from `xpict.future.spec`. After Rust-first, live ShadeSpec is generated from Rust; future keeps its richer ShadeSpec and must remain a **superset**.

## Target architecture

```
                    ┌─────────────────────────────┐
                    │  xpict-core (serde + TW)    │  LIVE SoT
                    │  DepictSpec, Edge*, Scene…  │
                    └──────────────┬──────────────┘
               typebridge generate │
         ┌─────────────┬───────────┴────────────┐
         ▼             ▼                        ▼
  js/src/generated/  python/xpict/          schema/*.json
  (TS interfaces)    contracts/_gen/        (live only)
                     (Pydantic)
         │             │
         ▼             ▼
  thin re-exports   thin re-exports +
  in api.ts etc.    validators / Strict extras
```

Future stays:

```
xpict.future (Pydantic) ──export_schema──► schema/future/xpict.schema.json
js/src/future/pict-types.ts               (design mirror, hand-maintained)
```

## Tooling choices

- **Crate:** `typebridge` with features `typescript`, `python`, `json_schema` only (drop Go/Swift/… noise).  
- **CLI:** `typebridge-cli` for `generate --all` and `check --ci` (drift gate). Prefer **explicit generate** in `make types` over silent build-script writes (reproducible CI, clear PR diffs).  
- **Config:** root `typewriter.toml` pointing outputs at:
  - `js/src/generated/` (or `js/src/contracts/`)
  - `python/xpict/contracts/_gen/`
  - `schema/` for live JSON Schema  
- **Zod:** default typebridge Zod siblings are **off** unless we adopt Zod runtime validation later (`[typescript] zod = false`).

## Phased delivery

### Phase 0 — Spike (1 PR, throwaway OK)

Goal: prove typebridge handles our hard shapes before ripping contracts.

Annotate a **small slice** in a throwaway feature or `#[cfg]` / dedicated `contracts` module:

1. `AlignToSpec` + untagged `AlignTo`  
2. Tagged `DepictSpec` (`#[serde(tag = "type", rename_all = "lowercase")]`)  
3. `ShadeSpec` with defaults / `skip_serializing_if`  
4. One `EdgeTask` / `MolTemplate` (nested + tagged enum)

Check:

- Generated Pydantic round-trips the same JSON fixtures as current `contracts`  
- Generated TS compiles with existing `depict` call sites  
- JSON Schema is usable (or we keep a thin post-process minify like today)  
- `#[serde(rename = "ref")]` / `rename = "type"` / `cls` land correctly  
- Untagged unions and `Vec<Option<String>>` (star_labels) emit sanely  

**Exit:** short spike notes in this doc (or PR description); go/no-go on typebridge vs fallback (schemars + datamodel-codegen / openapi-typescript).

### Phase 1 — Live wire types (EdgePlan + Scene)

Highest leverage: hosts already pass JSON through core.

1. Mark `edge.rs` + `scene.rs` public serde types with `TypeWriter` + `#[sync_to(typescript, python, json_schema)]`.  
2. Generate into `_gen` / `generated`.  
3. Replace hand `contracts/edge.py`, `contracts/scene.py` with re-exports from `_gen` (keep package import paths stable: `xpict.contracts.EdgePlan`, …).  
4. Replace JS `edge-plan.ts` / `scene-svg.ts` **type** definitions with imports from generated; keep runtime helpers hand-written.  
5. Commit regenerated `schema/edge-*.json`, `scene.schema.json`.  
6. CI: `make types-check` → `typebridge check --ci`.  
7. Drop those models from `export_schema.py` live list (or make export a no-op copy of committed Rust schemas for live).

### Phase 2 — DepictSpec / MolNode

1. Annotate `doc.rs` live document types.  
2. Generate Python + TS; thin wrappers if we need RootModel / validators that typebridge does not emit.  
3. **Strictness gap:** today Pydantic uses `extra="forbid"` and Field constraints (`scale > 0`, `weight >= 2/3`). Options:
   - Prefer Rust validation in `plan_edge` / `render_doc` (source of truth for behavior).  
   - Thin Python subclass / `__init_subclass__` model_validator on top of generated bases.  
   - Accept looser generated models + document that invalid docs fail at core.  
4. Live `ShadeSpec`: define fully in Rust; stop importing live shade from `xpict.future`. Future ShadeSpec remains a superset.  
5. Update `schema/xpict.schema.json`; keep “live ⊆ future” test (validate fixtures against both).

### Phase 3 — MoleculeIn + layout cleanup

1. Expose `MoleculeIn` (and related atom/bond structs) as first-class generated types in Py + JS (today Py often uses open dicts).  
2. **LayoutResult:** either  
   - (A) add a Rust `layout` contract if it is still a shared ABI, or  
   - (B) demote to Python-only host helper and stop calling it a live cross-language contract.  
   Prefer (B) if only Python layout backends use it; JS already uses a different single-mol shape.

### Phase 4 — Retire Python as live schema SoT

1. `make schema` → regenerate **future** only from Pydantic; live schemas from typebridge.  
2. `export_schema.py` shrinks to future + optional minify helpers.  
3. Docs (`docs/contracts.md`, `AGENTS.md`, `Makefile`) describe Rust-first live + future Pydantic.  
4. Optional: remove duplicate Field descriptions from Python once rustdoc / schema descriptions are enough.

## Repo / CI / Make

| Target | Behavior |
| --- | --- |
| `make types` | `typebridge generate --all` (or scoped paths) |
| `make types-check` | `typebridge check --ci` |
| `make schema` | live: types + copy/minify; future: existing Pydantic export |
| CI rust job | after tests, `make types-check` (and fail if generated files dirty) |

Do **not** require typebridge on Windows smoke; Linux CI is enough for drift.

Agents: extend `AGENTS.md` — edit live contracts in Rust, then `make types`.

## What stays hand-written

- Host logic: `process_edge_plan`, RDKit layout, SVG serializers  
- Single-mol client types that are **not** JSON ABI (`Mol`, `Rendered`, JS `AlignTarget` as Mol|Rendered)  
- Future PictSpec / `js/src/future/pict-types.ts`  
- Wasm glue still takes JSON strings; generated TS types document the payloads

## Risks and mitigations

| Risk | Mitigation |
| --- | --- |
| Young toolchain (typebridge 1.x) | Phase 0 spike; pin exact version; fallback path documented |
| Generated Pydantic ≠ StrictModel / Field meta | Thin wrappers; push hard validation into Rust |
| Output paths / build pollution | Explicit CLI generate + committed artifacts; no surprise writes in `cargo test` |
| DepictSpec enum vs MolNode struct duplication in Rust | Keep as-is for serde; ensure both annotated; consider consolidating later |
| Live ⊆ future breaks when Rust adds fields | CI test: live fixtures validate as `PictSpec`; when graduating, update future first or same PR |
| Minify / `factor_node_common_allof` only for future | Keep minify pipeline for future; live schemas may be flatter from typebridge — OK if committed and stable |

## Non-goals (this plan)

- Generating Go/Swift/Kotlin/GraphQL  
- Moving future PictSpec into Rust (until a feature graduates)  
- Replacing wasm-bindgen `.d.ts` (still stringly JSON FFI)  
- Auto-publishing schemas as a separate package

## Suggested PR sequence

1. **Spike** — annotate 2–3 types, sample outputs in a branch, write go/no-go  
2. **Infra** — `typewriter.toml`, `make types` / `types-check`, CI gate (empty or EdgePlan-only)  
3. **Edge + Scene** — first production cutover  
4. **DepictSpec** — document path  
5. **Schema SoT flip** — export_schema future-only; docs  

## Open questions (decide in spike)

1. Commit generated files vs generate in CI only? **Recommend commit** (JS/Python consumers and Pages don’t need typebridge installed).  
2. Package layout: `contracts/_gen/*` re-exported vs overwrite `contracts/*.py`? **Recommend `_gen` + stable re-exports** so wrappers can exist.  
3. Is LayoutResult cross-language live ABI? Decide Phase 3 A vs B.  
4. Accept typebridge JSON Schema as-is, or keep a minify pass for live?
