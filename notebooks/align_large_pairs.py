"""Default ``+`` align on larger mols and macrocycles.

Prints mapped-atom overlay stats, writes SVGs/PNGs under artifacts/align_large/.
"""

from __future__ import annotations

import json
import math
import subprocess
import sys
import time
from pathlib import Path

from xpict import mol
from xpict.contracts.depict import DepictSpec
from xpict.display import render_depict_mols

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "artifacts" / "align_large"
OUT.mkdir(parents=True, exist_ok=True)

# Related pairs: shared scaffold or ring, growing size. Stereo stripped so
# Chematic parse is not the variable.
PAIRS: list[tuple[str, str, str, str, str]] = [
    (
        "morphine_codeine",
        "morphine",
        "CN1CCC23c4c5ccc(O)c4OC2C(O)C=CC3C1C5",
        "codeine",
        "CN1CCC23c4c5ccc(OC)c4OC2C(O)C=CC3C1C5",
    ),
    (
        "cholesterol_testosterone",
        "cholesterol",
        "CC(C)CCCC(C)C1CCC2C3CC=C4CC(O)CCC4(C)C3CCC12C",
        "testosterone",
        "CC12CCC3C(CCC4=CC(=O)CCC34C)C1CCC2O",
    ),
    (
        "crown6_aza",
        "18-crown-6",
        "C1COCCOCCOCCOCCOCCO1",
        "aza-18-crown-6",
        "C1COCCOCCOCCNCCOCCO1",
    ),
    (
        "crown6_dibenzo",
        "18-crown-6",
        "C1COCCOCCOCCOCCOCCO1",
        "dibenzo-18-crown-6",
        "O1CCOCCOC2=CC=CC=C2OCCOCCOC3=CC=CC=C13",
    ),
    (
        "erythro_clarithro",
        "erythromycin",
        "CCC1OC(=O)C(C)C(OC2CC(C)(OC)C(O)C(C)O2)C(C)C(OC2OC(C)CC(N(C)C)C2O)C(C)(O)CC(C)C(=O)C(C)C(O)C1(C)O",
        "clarithromycin",
        "CCC1OC(=O)C(C)C(OC2CC(C)(OC)C(O)C(C)O2)C(C)C(OC2OC(C)CC(N(C)C)C2O)C(C)(OC)CC(C)C(=O)C(C)C(O)C1(C)O",
    ),
    (
        "erythro_azithro",
        "erythromycin",
        "CCC1OC(=O)C(C)C(OC2CC(C)(OC)C(O)C(C)O2)C(C)C(OC2OC(C)CC(N(C)C)C2O)C(C)(O)CC(C)C(=O)C(C)C(O)C1(C)O",
        "azithromycin",
        "CCC1CC(C)(O)C(O)C(C)N(C)CC(C)CC(C)(O)C(OC2OC(C)CC(N(C)C)C2O)C(C)C(OC2CC(C)(OC)C(O)C(C)O2)C(C)C(=O)O1",
    ),
    (
        "crown4_crown5",
        "12-crown-4",
        "C1COCCOCCOCCO1",
        "15-crown-5",
        "C1COCCOCCOCCOCCO1",
    ),
    (
        "c12_c15",
        "cyclododecane",
        "C1CCCCCCCCCCC1",
        "cyclopentadecane",
        "C1CCCCCCCCCCCCCC1",
    ),
    (
        "hq_quinone",
        "hydroquinone",
        "Oc1ccc(O)cc1",
        "benzoquinone",
        "O=C1C=CC(=O)C=C1",
    ),
    (
        "apap_napqi",
        "APAP",
        "CC(=O)Nc1ccc(O)cc1",
        "NAPQI",
        "CC(=O)N=C1C=CC(=O)C=C1",
    ),
    (
        "benzene_oxide",
        "benzene",
        "c1ccccc1",
        "benzene-oxide",
        "C1=CC2OC2C=C1",
    ),
    (
        "oxide_phenol",
        "benzene-oxide",
        "C1=CC2OC2C=C1",
        "phenol",
        "c1ccccc1O",
    ),
]


def with_debug(spec: DepictSpec) -> DepictSpec:
    d = spec.model_dump(exclude_none=True)
    d["debug_align"] = True
    return DepictSpec.model_validate(d)


def overlay_px(prev, cur) -> dict:
    dbg = cur.molecule.align_debug
    tags = list(dbg.tags) if dbg is not None else []
    if not tags:
        return {"n_map": 0, "median_px": None, "max_px": None}
    pref = {int(a["index"]): a for a in json.loads(prev.frame_molblock)["atoms"]}
    cref = {int(a["index"]): a for a in json.loads(cur.frame_molblock)["atoms"]}
    dists = []
    for q, r in tags:
        qa, ra = cref.get(int(q)), pref.get(int(r))
        if qa is None or ra is None:
            continue
        dists.append(math.hypot(qa["x"] - ra["x"], qa["y"] - ra["y"]))
    dists.sort()
    med = dists[len(dists) // 2] if dists else None
    return {
        "n_map": len(tags),
        "median_px": med,
        "max_px": max(dists) if dists else None,
    }


def svg_to_png(svg_path: Path, png_path: Path, width: int = 520) -> None:
    subprocess.run(
        ["rsvg-convert", "-w", str(width), str(svg_path), "-o", str(png_path)],
        check=True,
    )


def run_pair(key: str, n1: str, s1: str, n2: str, s2: str, *, mode: str | None) -> dict:
    if mode is None:
        spec = mol(s1, name=n1) + mol(s2, name=n2)
        spec = with_debug(spec)
        label = "default"
    else:
        spec = DepictSpec.model_validate(
            {
                "type": "group",
                "debug_align": True,
                "children": [
                    {"type": "mol", "id": "a", "smiles": s1, "name": n1},
                    {
                        "type": "mol",
                        "id": "b",
                        "smiles": s2,
                        "name": n2,
                        "align_to": {"ref": "a", "mode": mode},
                    },
                ],
            }
        )
        label = mode
    t0 = time.perf_counter()
    rows = render_depict_mols(spec)
    elapsed_ms = (time.perf_counter() - t0) * 1000
    assert len(rows) == 2, key
    stats = overlay_px(rows[0], rows[1])
    n_ref = len(rows[0].molecule.atoms)
    n_q = len(rows[1].molecule.atoms)
    stem = f"{key}_{label}"
    for i, row in enumerate(rows):
        svg_path = OUT / f"{stem}_{i}.svg"
        png_path = OUT / f"{stem}_{i}.png"
        svg_path.write_text(row.to_svg(), encoding="utf-8")
        svg_to_png(svg_path, png_path)
    montage = OUT / f"{stem}.png"
    subprocess.run(
        [
            "convert",
            str(OUT / f"{stem}_0.png"),
            str(OUT / f"{stem}_1.png"),
            "+append",
            str(montage),
        ],
        check=True,
    )
    rec = {
        "key": key,
        "mode": label,
        "n_ref": n_ref,
        "n_query": n_q,
        "n_map": stats["n_map"],
        "cover": stats["n_map"] / n_q if n_q else 0,
        "median_px": stats["median_px"],
        "max_px": stats["max_px"],
        "elapsed_ms": elapsed_ms,
        "png": str(montage.relative_to(ROOT)),
    }
    return rec


def main() -> int:
    pairs = PAIRS
    print(
        f"{'pair':<24} {'mode':<8} {'n':>7} {'map':>4} {'cover':>6} "
        f"{'medpx':>8} {'ms':>8}",
        flush=True,
    )
    rows = []
    for key, n1, s1, n2, s2 in pairs:
        for mode in (None, "skip"):
            try:
                rec = run_pair(key, n1, s1, n2, s2, mode=mode)
            except Exception as e:  # noqa: BLE001 — demo: keep going
                print(f"{key:<24} {mode or 'default':<8} FAIL {e}", flush=True)
                continue
            med = rec["median_px"]
            med_s = f"{med:8.2f}" if med is not None else f"{'—':>8}"
            print(
                f"{rec['key']:<24} {rec['mode']:<8} "
                f"{rec['n_ref']:>3}/{rec['n_query']:<3} "
                f"{rec['n_map']:>4} {rec['cover']:6.2f} {med_s} "
                f"{rec['elapsed_ms']:8.0f}",
                flush=True,
            )
            rows.append(rec)
    (OUT / "stats.json").write_text(json.dumps(rows, indent=2), encoding="utf-8")
    print(f"wrote {OUT / 'stats.json'}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
