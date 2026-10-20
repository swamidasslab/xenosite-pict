# TODO

- Spoke bonds: molfile ENDPTS / haptic CX `C:` (ATTACH=ALL); CX `m:` ring-center stems are in core
- Align pose lock: atom maps for fused scaffolds / metabolites / erythro–azithro are element-true; CoordGen soft-constrain still rotates morphine-like pentacycles (skip already shares the pose)
- Constraint layout: richer discrete side flips; tune kit strengths from notebook visuals; cap route-point birth harder on dense mols
- Lift remaining legacy Pict gaps onto live DepictSpec (titles, rgroups/annots, radical dots) or delete those tests
- JS host still uses RDKit edge processor — fold onto the same in-core Chematic+CoordGen path as Python/Rust
- Move Scene→SVG into Rust; retire host serializer in `rendered.py` when bindings expose it
- Fold `api.py` / `future/` off the live import surface or type them without `Any`/`object`
