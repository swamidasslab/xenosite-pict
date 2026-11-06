# Engine constants: promote vs internal

Maintainer policy for “should this constant be in DepictSpec?”.
Not published (`exclude_docs: dev/**`).

## Rubric

**Promote into DepictSpec** when all of:

- Authors would expect different values in different document contexts
- The name is backend-agnostic (not ELK strategy strings, not AL ρ growth)
- Changing it changes published look / layout intent, not only search quality or runtime
- We will support it across Python / JS via `make types`

**Keep Rust-only (`EngineConfig`)** when any of:

- Tuning / search / solver control
- Route score weights, visibility caps, reopt budgets, network-size cutoffs
- Raw backend pass-through (no live `elk_options` dict)

**Leave in [`metrics.rs`](../../crates/xpict-core/src/metrics.rs)** when the value is part of the coherent house-style set (`STROKE_FRAC`, `WEDGE_WIDTH_FRAC`, …). Authors already steer ink via semantic overrides (`scale`, `weight`, `pad`).

## Inventory (scheme + solver + paint)

| Constant / group | Location | Tag | Notes |
| --- | --- | --- | --- |
| `LayoutOpts` spacing / direction / routing | `spec.rs` | **promoted** | Public scheme layout |
| `MolOpts` / `CommonOpts` (`color`, `scale`, `weight`, `pad`, …) | `spec.rs` | **promoted** | Cascading paint |
| `RegionSpec.padding`, `CircleSpec.scale`, `MolTextSpec.margin`/`scale` | `chrome.rs` | **promoted** | Annotation / caption geometry |
| `TextNode.margin` | `spec.rs` | **promoted** | Caption refs via `label` id (parity with `name`) |
| `LayoutOpts.quality` (`pretty` \| `fast`) | `spec.rs` | **promoted** | Maps to `EngineConfig` presets; not coefficients |
| Route penalties, vis caps, reopt | `scheme_route.rs` → `EngineConfig::route` | **internal** | |
| `SolveOpts` defaults + AL knobs | `layout_constraints.rs` → `EngineConfig::solve` | **internal** | |
| Arrow kink / turn / head / dash | `arrows.rs` → `EngineConfig::arrow` | **internal** | Pub house consts remain aliases of defaults |
| Scheme ink standoff, coterminal, network caps, label font | `scheme_layout.rs` → `EngineConfig::scheme` | **internal** | Standoff stays a pub const for tests |
| Nudge max / steps | `scheme_nudge.rs` → `EngineConfig::nudge` | **internal** | |
| ELK strategy strings / edgeEdge spacing | `elk.rs` → `EngineConfig::elk` | **internal** | |
| `SCALE` / `FONT_FRAC` / `STROKE_FRAC` / shade fractions | `metrics.rs` | **metrics** | House style set |

## Surfaces

```
DepictSpec  ──►  xpict-core algorithms  ◄──  EngineConfig (Rust only)
```

- No host-facing `render(spec, engine_opts)` side channel
- No generic `engine: {…}` bag on DepictSpec
- Tests / ablations may construct `EngineConfig` in-crate; bindings do not export it
