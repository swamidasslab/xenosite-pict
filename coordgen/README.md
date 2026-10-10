# CoordGen sketch (superseded)

Draft `ffi.*` targeted APIs that **do not exist** in Schrödinger coordgenlibs
(`setForcePlanar`, `addExtraBoundingBox`, `getParameters().templateStrength…`,
`getCollisionScore`, `CoordgenMinimizer::run(molecule)`, …).

The live integration is:

```text
crates/coordgen-sys/   # corrected C ABI + safe Rust
xpict._native.generate_2d_coordinates  # Python binding (depict path uses core)
```

## Real options we wire

| Flag / field | Upstream |
| --- | --- |
| `LAYOUT_FLAG_EVEN_ANGLES` | `setEvenAngles(true)` |
| `LAYOUT_FLAG_QUICK_PRECISION` | ctor `SKETCHER_QUICK_PRECISION` |
| `LAYOUT_FLAG_BEST_PRECISION` | ctor `SKETCHER_BEST_PRECISION` |
| `LAYOUT_FLAG_FORCE_OPEN_MACROCYCLES` | `setForceOpenMacrocycles(true)` |
| `LAYOUT_FLAG_METAL_BONDS_AS_ZOB` | `setTreatNonterminalBondsToMetalAsZOBs(true)` |
| `LAYOUT_FLAG_SKIP_MINIMIZATION` | `setSkipMinimization(true)` |
| fixed / constrained + `input_x/y` | `atom->fixed` / `constrained` + `templateCoordinates` |
| bond stereo + stereo atoms | `setStereoChemistry` + `setAbsoluteStereoFromStereoInfo` |

Compiled-in ring templates (`CoordgenTemplates.cpp`) are used; maeparser
runtime templates are not linked.

Not exposed (no upstream API): per-atom extra boxes, template/overlap
multipliers, collision score, atom CIP chirality helpers (can add later).
