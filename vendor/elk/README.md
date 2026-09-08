# ELK packaging hook

Python multi-molecule diagram layout will ship the **ELK JAR** (and/or elkjs) as package data and invoke it through a **V8 bridge** (e.g. STPyV8). Web `js/` uses **elkjs** directly — no JAR.

## Layout (intended)

```
vendor/elk/
  README.md          # this file
  elk.jar            # optional vendored JAR (not committed yet; download at package build)
  fetch_elk.sh       # helper to place elk.jar here
```

At runtime, `xenosite.pict.diagram.elk` will:

1. Prefer a V8 + elkjs path (same algorithm surface as the browser).
2. Fall back to the vendored JAR via the bridge when configured.
3. Until the bridge is wired, use **grid/row** placement and emit `PictBackendWarning`.

## Fetch (manual / CI)

```bash
./vendor/elk/fetch_elk.sh
```

Do not commit large JAR binaries unless the release process explicitly vendors them into the wheel.
