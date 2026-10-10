# TODO

- Retire host `align.py` once `test_charge_star_align` moves onto Rust depict (align is not on the ship path)
- Lift remaining legacy Pict gaps onto live DepictSpec (titles, rgroups/annots, radical dots) or delete those tests
- JS host still uses RDKit edge processor — fold onto the same in-core Chematic+CoordGen path as Python/Rust
- Move Scene→SVG into Rust; retire Python `draw/` from the ship path (keep as optional)
- Fold `api.py` / `future/` off the live import surface or type them without `Any`/`object`
