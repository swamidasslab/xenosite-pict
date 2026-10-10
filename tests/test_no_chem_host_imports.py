"""Meta: Python package must not import RDKit, Indigo, or Chematic.

Parse/layout live in Rust (Chematic + CoordGen). Hosts are adapters only.
"""

from __future__ import annotations

import ast
from pathlib import Path

import pytest

FORBIDDEN = frozenset({"rdkit", "indigo", "chematic", "chematics"})

_PKG = Path(__file__).resolve().parents[1] / "python" / "xpict"


def _top_level(name: str) -> str:
    return name.split(".", 1)[0]


def _forbidden_imports(path: Path) -> list[str]:
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    hits: list[str] = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                if _top_level(alias.name) in FORBIDDEN:
                    hits.append(f"{path.name}:{node.lineno}: import {alias.name}")
        elif isinstance(node, ast.ImportFrom) and node.module:
            if _top_level(node.module) in FORBIDDEN:
                hits.append(f"{path.name}:{node.lineno}: from {node.module} import …")
    return hits


def test_python_package_has_no_rdkit_indigo_chematic_imports():
    py_files = sorted(_PKG.rglob("*.py"))
    assert py_files, f"no Python files under {_PKG}"
    offenders: list[str] = []
    for path in py_files:
        offenders.extend(_forbidden_imports(path))
    if offenders:
        pytest.fail(
            "forbidden cheminformatics imports in python/xpict "
            "(use Rust depict instead):\n  " + "\n  ".join(offenders)
        )
