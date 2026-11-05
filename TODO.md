# TODO

- Chem tree still uses bond centerlines for stereo wedges and Kekulé offset strokes
- Raise xpict-core llvm-cov back toward 97% (caption, chrome_paint, layout_constraints)
- Spoke bonds: molfile ENDPTS / haptic CX `C:` (ATTACH=ALL); CX `m:` ring-center stems are in core
- Align pose lock: atom maps for fused scaffolds / metabolites / erythro–azithro are element-true; CoordGen soft-constrain still rotates morphine-like pentacycles (skip already shares the pose)
- Constraint layout: richer discrete side flips; tune kit strengths from notebook visuals; cap route-point birth harder on dense mols
- Lift remaining legacy Pict gaps onto live DepictSpec (titles, rgroups/annots) or delete those tests
- Fold `api.py` / `future/` off the live import surface or type them without `Any`/`object`
