# TODO

- Align pose lock: MCS maps large related mols, but CoordGen soft-constrain often rotates or collapses the query (morphine/codeine, mismatched macrocycles)
- Constraint layout: richer discrete side flips; tune kit strengths from notebook visuals; cap route-point birth harder on dense mols
- Lift remaining legacy Pict gaps onto live DepictSpec (titles, rgroups/annots, radical dots) or delete those tests
- JS host still uses RDKit edge processor — fold onto the same in-core Chematic+CoordGen path as Python/Rust
- Move Scene→SVG into Rust; retire host serializer in `rendered.py` when bindings expose it
- Fold `api.py` / `future/` off the live import surface or type them without `Any`/`object`
