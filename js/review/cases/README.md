# Review cases

Each file is one depiction example. The local gallery (`make review`) loads
this directory; it does not hard-code structures in the React app.

```
cases/<group>/<id>.yaml
```

| Piece | Meaning |
| --- | --- |
| `<group>` | Sidebar section (`structure`, `chrome`, `scheme`, …) |
| `<id>` | Snapshot / notes key. Must be unique across all groups. |
| `title` | Short heading |
| `expect` | What a reviewer should see (the “key behavior”) |
| `spec` | Live `DepictSpec` (`mol` / `group` / `reaction_scheme` / `grid`) |

YAML comments (`#`) are allowed. JSON (`.json`) works too.

```yaml
title: Ethanol
expect: Open-chain carbon skeleton with an OH label (implicit H).
spec:
  type: mol
  smiles: CCO
```

Add a case by adding a file. `npm run test:catalog` paints every spec through
the JS/WASM library.
