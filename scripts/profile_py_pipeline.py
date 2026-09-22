#!/usr/bin/env python3
"""Profile Python depiction before/after JS-mirror migration.

Usage:
  python scripts/profile_py_pipeline.py --label before
  python scripts/profile_py_pipeline.py --label after
"""

from __future__ import annotations

import argparse
import json
import statistics
import time
from pathlib import Path

CASES = [
    ("benzene", {"molecules": [{"smiles": "c1ccccc1"}]}),
    ("phenol", {"molecules": [{"smiles": "c1ccccc1O"}]}),
    ("aspirin", {"molecules": [{"smiles": "CC(=O)Oc1ccccc1C(=O)O"}]}),
    ("star_cx", {"molecules": [{"cxsmiles": "*C1C=C(O)C=CC1=O |$GSH;;;;;;;;$|"}]}),
    ("multi_star", {"molecules": [{"cxsmiles": "*C* |$;;R2;$|"}]}),
]

WARMUP = 3
ITERS = 25
OUT_DIR = Path("/opt/cursor/artifacts")


def _stats(samples: list[float]) -> dict[str, float]:
    s = sorted(samples)
    return {
        "mean": statistics.fmean(s),
        "p50": s[len(s) // 2],
        "p95": s[int(len(s) * 0.95)],
        "min": s[0],
        "max": s[-1],
    }


def profile_legacy(backend: str | None) -> dict:
    from xpict import Pict, render

    rows = []
    for cid, spec in CASES:
        pict = Pict(backend=backend)
        for _ in range(WARMUP):
            pict.render(spec)
        times = []
        for _ in range(ITERS):
            t0 = time.perf_counter()
            pict.render(spec)
            times.append((time.perf_counter() - t0) * 1000.0)
        # also shorthand
        shorthand = []
        for _ in range(ITERS):
            t0 = time.perf_counter()
            render(spec, backend=backend)
            shorthand.append((time.perf_counter() - t0) * 1000.0)
        rows.append(
            {
                "id": cid,
                "pict_render_ms": _stats(times),
                "render_ms": _stats(shorthand),
            }
        )
    return {"kind": "py_legacy_profile", "backend": backend or "auto", "rows": rows}


def profile_mirror() -> dict:
    """JS-mirrored API: xpict.mol / render / to_svg via Rust depict."""
    from xpict.client import mol, render as js_render, to_svg

    rows = []
    for cid, spec in CASES:
        # map legacy case → smiles/cx string
        m0 = spec["molecules"][0]
        source = m0.get("cxsmiles") or m0.get("smiles")
        assert source
        for _ in range(WARMUP):
            r = js_render(mol(source))
            to_svg(r.scene)
        times = []
        for _ in range(ITERS):
            t0 = time.perf_counter()
            r = js_render(mol(source))
            to_svg(r.scene)
            times.append((time.perf_counter() - t0) * 1000.0)
        rows.append({"id": cid, "mirror_api_ms": _stats(times)})
    return {"kind": "py_mirror_profile", "rows": rows}


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--label", required=True, choices=("before", "after", "mirror"))
    ap.add_argument("--backend", default=None)
    args = ap.parse_args()

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    if args.label == "mirror":
        payload = profile_mirror()
    else:
        payload = profile_legacy(args.backend)
    payload["label"] = args.label
    payload["iters"] = ITERS
    payload["warmup"] = WARMUP
    path = OUT_DIR / f"py_pipeline_profile_{args.label}.json"
    path.write_text(json.dumps(payload, indent=2) + "\n")
    print(f"wrote {path}")
    # table
    print(f"{'case':12} {'mean_ms':>10}")
    for row in payload["rows"]:
        key = "mirror_api_ms" if "mirror_api_ms" in row else "pict_render_ms"
        print(f"{row['id']:12} {row[key]['mean']:10.2f}")


if __name__ == "__main__":
    main()
