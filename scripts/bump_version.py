#!/usr/bin/env python3
"""Set every public package version to the same semver (minor/major cuts).

Surfaces updated (must stay in lockstep for X.Y):
  - js/package.json + js/package-lock.json
  - pyproject.toml
  - python/xpict/__init__.py ``__version__``
  - crates/xpict-core, xpict, xpict-py, xpict-wasm Cargo.toml
  - crates/xpict → xpict-core path dep version

Usage:
  python3 scripts/bump_version.py 0.2.0
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SEMVER = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+([.-].*)?$")


def set_toml_version(path: Path, ver: str) -> None:
    text = path.read_text(encoding="utf-8")
    text, n = re.subn(r'(?m)^version = "[^"]+"', f'version = "{ver}"', text, count=1)
    if n != 1:
        raise SystemExit(f"version line not found in {path}")
    path.write_text(text, encoding="utf-8")


def set_js(ver: str) -> None:
    pkg = ROOT / "js" / "package.json"
    data = json.loads(pkg.read_text(encoding="utf-8"))
    data["version"] = ver
    pkg.write_text(
        json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )

    lock = ROOT / "js" / "package-lock.json"
    if lock.is_file():
        lock_data = json.loads(lock.read_text(encoding="utf-8"))
        lock_data["version"] = ver
        packages = lock_data.get("packages")
        if isinstance(packages, dict) and "" in packages:
            packages[""]["version"] = ver
        lock.write_text(
            json.dumps(lock_data, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )


def set_python_dunder(ver: str) -> None:
    init = ROOT / "python" / "xpict" / "__init__.py"
    text = init.read_text(encoding="utf-8")
    text, n = re.subn(
        r'(?m)^__version__ = "[^"]+"',
        f'__version__ = "{ver}"',
        text,
        count=1,
    )
    if n != 1:
        raise SystemExit(f"__version__ not found in {init}")
    init.write_text(text, encoding="utf-8")


def set_xpict_core_dep(ver: str) -> None:
    dep = ROOT / "crates" / "xpict" / "Cargo.toml"
    text = dep.read_text(encoding="utf-8")
    text, n = re.subn(
        r'(xpict-core = \{ path = "[^"]+", version = ")[^"]+(")',
        rf"\g<1>{ver}\2",
        text,
        count=1,
    )
    if n != 1:
        raise SystemExit(f"xpict-core version dep not found in {dep}")
    dep.write_text(text, encoding="utf-8")


def bump(ver: str) -> None:
    if not SEMVER.match(ver):
        raise SystemExit(f"not semver X.Y.Z: {ver}")

    set_js(ver)
    set_toml_version(ROOT / "pyproject.toml", ver)
    set_python_dunder(ver)
    for name in ("xpict-core", "xpict", "xpict-py", "xpict-wasm"):
        set_toml_version(ROOT / "crates" / name / "Cargo.toml", ver)
    set_xpict_core_dep(ver)
    print(f"all packages → {ver}")


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(__doc__.strip())
    bump(sys.argv[1])


if __name__ == "__main__":
    main()
