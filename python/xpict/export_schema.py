"""Export Pydantic contracts to committed JSON Schema artifacts."""

from __future__ import annotations

import json
import re
from collections import defaultdict
from pathlib import Path
from typing import Any

from xpict.future.nodes import PictSpec

_REPO_SCHEMA = Path(__file__).resolve().parents[2] / "schema"
_FUTURE_SCHEMA = _REPO_SCHEMA / "future"

# Subtrees smaller than this (canonical JSON chars) are not worth extracting.
_MIN_DEDUPE_CHARS = 120

# JSON Schema keywords whose values must not be replaced by a bare $ref.
_NO_REF_VALUE = frozenset(
    {
        "oneOf",
        "anyOf",
        "allOf",
        "prefixItems",
        "enum",
        "required",
        "discriminator",
        "examples",
        "type",  # string or array of strings
    }
)


def schema_dir() -> Path:
    return _REPO_SCHEMA


def _canon(value: Any) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def _is_ref(value: Any) -> bool:
    return isinstance(value, dict) and set(value) <= {"$ref"} and "$ref" in value


def _looks_like_schema(node: dict[str, Any]) -> bool:
    """Heuristic: JSON Schema object (not a random annotation dict)."""
    markers = {
        "type",
        "properties",
        "items",
        "oneOf",
        "anyOf",
        "allOf",
        "$ref",
        "enum",
        "const",
        "additionalProperties",
        "prefixItems",
        "title",
        "description",
        "default",
        "discriminator",
    }
    return bool(markers & set(node))


def _walk_collect(node: Any, counts: dict[str, int], examples: dict[str, Any]) -> None:
    """Count duplicate dict subschemas only (never lists — oneOf etc. must stay arrays)."""
    match node:
        case dict():
            if _is_ref(node):
                return
            if _looks_like_schema(node):
                key = _canon(node)
                counts[key] += 1
                examples.setdefault(key, node)
            for v in node.values():
                _walk_collect(v, counts, examples)
        case list():
            for v in node:
                _walk_collect(v, counts, examples)
        case _:
            return


def _slug_for(subtree: Any, used: set[str]) -> str:
    title = None
    if isinstance(subtree, dict):
        title = subtree.get("title")
        if not title and subtree.get("type") == "array" and "items" in subtree:
            items = subtree["items"]
            if isinstance(items, dict) and (
                "discriminator" in items or "oneOf" in items or "$ref" in items
            ):
                title = "NodeList"
            else:
                title = "ArraySchema"
        elif not title and "discriminator" in subtree and "oneOf" in subtree:
            title = "NodeUnion"
        elif not title and "anyOf" in subtree and subtree.get("title"):
            title = subtree["title"]
    if isinstance(title, str) and title.strip():
        base = re.sub(r"[^A-Za-z0-9_]", "", title.strip()) or "Shared"
    else:
        base = "Shared"
    name = base
    n = 2
    while name in used:
        name = f"{base}{n}"
        n += 1
    used.add(name)
    return name


def minify_json_schema(schema: dict[str, Any]) -> dict[str, Any]:
    """Collapse duplicate subschemas into ``$defs`` + ``$ref``.

    Only identical **object** subschemas are factored (lists are left alone so
    ``oneOf`` / ``anyOf`` / ``discriminator`` stay valid). Existing ``$defs``
    bodies are reused when they match.
    """
    root: dict[str, Any] = json.loads(json.dumps(schema))
    defs: dict[str, Any] = dict(root.get("$defs") or {})

    counts: dict[str, int] = defaultdict(int)
    examples: dict[str, Any] = {}
    _walk_collect(root, counts, examples)

    canon_to_name: dict[str, str] = {_canon(body): name for name, body in defs.items()}
    used_names = set(defs)

    for key, freq in sorted(counts.items(), key=lambda kv: len(kv[0]), reverse=True):
        if freq < 2 or len(key) < _MIN_DEDUPE_CHARS:
            continue
        if key in canon_to_name:
            continue
        if examples[key] is root:
            continue
        name = _slug_for(examples[key], used_names)
        defs[name] = examples[key]
        canon_to_name[key] = name

    def rewrite(node: Any, path: tuple[Any, ...] = (), parent_key: str | None = None) -> Any:
        match node:
            case dict():
                if _is_ref(node):
                    return node
                key = _canon(node)
                name = canon_to_name.get(key)
                at_def_body = len(path) == 2 and path[0] == "$defs" and path[1] == name
                if name is not None and not at_def_body and parent_key not in _NO_REF_VALUE:
                    return {"$ref": f"#/$defs/{name}"}
                return {k: rewrite(v, (*path, k), parent_key=k) for k, v in node.items()}
            case list():
                return [rewrite(v, (*path, i), parent_key=parent_key) for i, v in enumerate(node)]
            case _:
                return node

    out = rewrite(root)
    assert isinstance(out, dict)
    out["$defs"] = {name: rewrite(body, ("$defs", name)) for name, body in defs.items()}
    return out


_LEAF_NODE_DEFS = frozenset(
    {
        "MolNode",
        "ArrowNode",
        "TextNode",
        "ImageNode",
        "TableNode",
        "RefNode",
        "AnnotationNode",
    }
)

_CONTAINER_NODE_DEFS = frozenset(
    {
        "GroupNode",
        "GridNode",
        "StackNode",
        "ReactionNode",
        "NetworkNode",
    }
)

# Properties owned by NodeCommon / ContainerCommon (shared via allOf).
_NODE_COMMON_FIELDS = ("id", "panel", "layout", "meta")
_CONTAINER_COMMON_FIELDS = ("children",)


def factor_node_common_allof(schema: dict[str, Any]) -> dict[str, Any]:
    """Rewrite node defs as ``allOf`` over ``NodeCommon`` / ``ContainerCommon``.

    Pydantic flattens Python inheritance into each model schema. JSON Schema's
    portable extension form is ``allOf`` + ``$ref``, which we restore here.
    Leaves extend ``NodeCommon``; containers extend ``ContainerCommon``
    (itself ``allOf`` ``NodeCommon`` + ``children``).
    """
    root: dict[str, Any] = json.loads(json.dumps(schema))
    defs: dict[str, Any] = dict(root.get("$defs") or {})

    # Templates: MolNode for common fields; GridNode for children.
    mol_props = dict((defs.get("MolNode") or {}).get("properties") or {})
    grid_props = dict((defs.get("GridNode") or {}).get("properties") or {})
    common_props = {k: mol_props[k] for k in _NODE_COMMON_FIELDS if k in mol_props}
    # Prefer children schema from a container template when present.
    children_prop = grid_props.get("children") or mol_props.get("children")
    if len(common_props) < 2:
        return root

    defs["NodeCommon"] = {
        "title": "NodeCommon",
        "description": "Fields shared by every figure node (id, panel, layout, meta).",
        "type": "object",
        "properties": common_props,
    }

    if children_prop is not None:
        defs["ContainerCommon"] = {
            "title": "ContainerCommon",
            "description": (
                "Node that owns nested children (group / grid / stack / reaction / network)."
            ),
            "allOf": [
                {"$ref": "#/$defs/NodeCommon"},
                {
                    "type": "object",
                    "properties": {"children": children_prop},
                },
            ],
        }

    def _rewrite_leaf(name: str) -> None:
        body = defs.get(name)
        if not isinstance(body, dict):
            return
        props = dict(body.get("properties") or {})
        if not all(f in props for f in common_props):
            return
        own = {k: v for k, v in props.items() if k not in common_props}
        rewritten: dict[str, Any] = {
            "title": body.get("title", name),
            "allOf": [
                {"$ref": "#/$defs/NodeCommon"},
                {"type": "object", "properties": own},
            ],
        }
        if "description" in body:
            rewritten["description"] = body["description"]
        if body.get("additionalProperties") is False:
            rewritten["unevaluatedProperties"] = False
        defs[name] = rewritten

    def _rewrite_container(name: str) -> None:
        body = defs.get(name)
        if not isinstance(body, dict):
            return
        props = dict(body.get("properties") or {})
        own = {
            k: v
            for k, v in props.items()
            if k not in common_props and k not in _CONTAINER_COMMON_FIELDS
        }
        base_ref = "#/$defs/ContainerCommon" if "ContainerCommon" in defs else "#/$defs/NodeCommon"
        extension: dict[str, Any] = {"type": "object", "properties": own}
        rewritten = {
            "title": body.get("title", name),
            "allOf": [
                {"$ref": base_ref},
                extension,
            ],
        }
        if "description" in body:
            rewritten["description"] = body["description"]
        if body.get("additionalProperties") is False:
            rewritten["unevaluatedProperties"] = False
        defs[name] = rewritten

    for name in _LEAF_NODE_DEFS:
        _rewrite_leaf(name)
    for name in _CONTAINER_NODE_DEFS:
        _rewrite_container(name)

    root["$defs"] = defs
    return root


def export_schemas(out_dir: Path | None = None, *, minify: bool = True) -> dict[str, Path]:
    """Write future PictSpec under ``schema/future/``; preserve Rust live schemas.

    Live ``xpict`` / edge / scene schemas are owned by ``make types`` (schemars).
    When those files already exist under ``out_dir`` (or the repo ``schema/``),
    they are listed in the return map but not rewritten.
    """
    target = out_dir or schema_dir()
    target.mkdir(parents=True, exist_ok=True)
    future_dir = (out_dir / "future") if out_dir is not None else _FUTURE_SCHEMA
    future_dir.mkdir(parents=True, exist_ok=True)

    # Live schemas are owned by Rust (schemars via ``make types``).
    # This export only (re)writes future PictSpec under ``schema/future/``.
    live_rust = (
        "xpict.schema.json",
        "edge-plan.schema.json",
        "edge-result.schema.json",
        "scene.schema.json",
    )
    future = {
        "xpict.schema.json": factor_node_common_allof(PictSpec.model_json_schema()),
    }

    written: dict[str, Path] = {}
    for rust_name in live_rust:
        rust_path = target / rust_name
        if rust_path.is_file():
            written[rust_name] = rust_path
    for name, schema in future.items():
        if minify:
            schema = minify_json_schema(schema)
        path = future_dir / name
        path.write_text(json.dumps(schema, indent=2) + "\n", encoding="utf-8")
        written[f"future/{name}"] = path
    return written


def main() -> None:
    raw = PictSpec.model_json_schema()
    factored = factor_node_common_allof(raw)
    mini = minify_json_schema(factored)
    raw_s = json.dumps(raw, indent=2)
    mini_s = json.dumps(mini, indent=2)
    print(
        f"future/xpict.schema.json  raw={len(raw_s.splitlines())} lines / {len(raw_s)} chars  "
        f"→ allOf+minify={len(mini_s.splitlines())} lines / {len(mini_s)} chars  "
        f"({100 * len(mini_s) / len(raw_s):.0f}% of raw)"
    )
    mol = mini["$defs"]["MolNode"]
    print("MolNode keys:", list(mol))
    print(json.dumps(mol, indent=2)[:600])
    assert "NodeCommon" in mini["$defs"]
    assert mol.get("allOf")
    for _, path in export_schemas().items():
        print(f"wrote {path}")


if __name__ == "__main__":
    main()
