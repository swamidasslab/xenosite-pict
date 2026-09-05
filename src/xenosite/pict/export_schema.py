"""Export Pydantic contracts to committed JSON Schema artifacts."""

from __future__ import annotations

import json
from pathlib import Path

from xenosite.pict.contracts.layout import LayoutResult
from xenosite.pict.contracts.scene import Scene
from xenosite.pict.contracts.spec import PictSpec

_REPO_SCHEMA = Path(__file__).resolve().parents[3] / "schema"


def schema_dir() -> Path:
    return _REPO_SCHEMA


def export_schemas(out_dir: Path | None = None) -> dict[str, Path]:
    target = out_dir or schema_dir()
    target.mkdir(parents=True, exist_ok=True)
    mapping = {
        "pict.schema.json": PictSpec.model_json_schema(),
        "layout.schema.json": LayoutResult.model_json_schema(),
        "scene.schema.json": Scene.model_json_schema(),
    }
    written: dict[str, Path] = {}
    for name, schema in mapping.items():
        path = target / name
        path.write_text(json.dumps(schema, indent=2) + "\n", encoding="utf-8")
        written[name] = path
    return written


def main() -> None:
    for name, path in export_schemas().items():
        print(f"wrote {path}")


if __name__ == "__main__":
    main()
