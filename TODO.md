# TODO

- Lift remaining legacy Pict gaps onto live DepictSpec (titles, rgroups/annots, radical dots) or delete those tests
- JS host still uses RDKit edge processor — fold onto the same in-core Chematic+CoordGen path as Python/Rust
- Move Scene→SVG into Rust; retire host serializer in `rendered.py` when bindings expose it
- Fold `api.py` / `future/` off the live import surface or type them without `Any`/`object`
