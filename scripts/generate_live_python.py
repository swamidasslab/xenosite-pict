#!/usr/bin/env python3
"""Emit live Pydantic StrictModels from schemars JSON Schema (Rust SoT).

Generic rules (no per-type name tables):

1. ``definitions`` / root objects → ``StrictModel`` (``extra=forbid``,
   ``model_dump(exclude_none=True)`` to match serde ``skip_serializing_if``).
2. string ``enum`` → ``Literal[...]`` alias.
3. ``oneOf`` of string enums only → merge into one ``Literal`` (schemars split).
4. ``oneOf`` of objects with a single-value ``kind``/``type`` enum → tagged
   variants. Name = reuse a def with the same disc value, else:
   - ``kind`` → ``{Pascal(value)}Prim``
   - ``type`` + parent ``FooBarBaz`` → ``{Pascal(value)}{BarBaz}`` (drop first
     CamelCase segment); parent ending in ``Spec`` → ``{Pascal(value)}Node``.
5. Root ``oneOf`` with a discriminator → ``RootModel``.
6. Non-required arrays → ``default_factory=list``; other optionals → ``| None``.
7. Fixed-length 2-item arrays → ``tuple[T, U]``.
8. Disc fields (``type``/``kind``) whose schema is a single-value string enum
   (inline or ``$ref``) → ``Literal['v'] = 'v'``.

Modules are driven only by schema files + titles (see ``MODULES``). Host
helpers (``mols``, ``MolSpec``) live outside this emitter.

Invoked by ``scripts/generate_live_types.sh`` / ``make types``.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = ROOT / "schema"
OUT = ROOT / "python" / "xpict" / "contracts"

# schema file(s) → output module. Multiple files merge definitions and emit
# each schema's ``title`` as a root (EdgePlan + EdgeResult).
MODULES: list[tuple[str, tuple[str, ...], str]] = [
    (
        "scene.py",
        ("scene.schema.json",),
        "Drawable scene graph — live ABI from xpict-core (schemars).",
    ),
    (
        "edge.py",
        ("edge-plan.schema.json", "edge-result.schema.json"),
        "EdgePlan / EdgeResult — live ABI from xpict-core (schemars).",
    ),
    (
        "depict.py",
        ("xpict.schema.json",),
        "DepictSpec — live document ABI from xpict-core (schemars).",
    ),
]

STRICT_MODEL = '''\
class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")

    def model_dump(self, *args, **kwargs):
        kwargs.setdefault("exclude_none", True)
        return super().model_dump(*args, **kwargs)

    def model_dump_json(self, *args, **kwargs):
        kwargs.setdefault("exclude_none", True)
        return super().model_dump_json(*args, **kwargs)
'''


def _load(name: str) -> dict[str, Any]:
    return json.loads((SCHEMA / name).read_text(encoding="utf-8"))


def _defs(schema: dict[str, Any]) -> dict[str, Any]:
    return dict(schema.get("definitions") or schema.get("$defs") or {})


def _ref(node: dict[str, Any]) -> str | None:
    r = node.get("$ref")
    return r.rsplit("/", 1)[-1] if isinstance(r, str) else None


def _pascal(s: str) -> str:
    return "".join(p.capitalize() for p in s.split("_"))


def _camel_parts(name: str) -> list[str]:
    return re.findall(r"[A-Z][a-z0-9]*", name)


def _single_str_enum(node: dict[str, Any] | None) -> str | None:
    if not node:
        return None
    if isinstance(node.get("enum"), list) and len(node["enum"]) == 1:
        v = node["enum"][0]
        return str(v) if isinstance(v, str) else None
    if isinstance(node.get("const"), str):
        return node["const"]
    return None


def _is_null(node: dict[str, Any]) -> bool:
    return node.get("type") == "null"


def _unwrap(node: dict[str, Any]) -> tuple[dict[str, Any], bool]:
    """Strip allOf-singletons and nullability; return (inner, nullable)."""
    if "allOf" in node and len(node["allOf"]) == 1 and isinstance(node["allOf"][0], dict):
        merged = {k: v for k, v in node.items() if k != "allOf"}
        merged.update(node["allOf"][0])
        return _unwrap(merged)
    if "anyOf" in node:
        parts = [p for p in node["anyOf"] if not _is_null(p)]
        nulls = [p for p in node["anyOf"] if _is_null(p)]
        if len(parts) == 1 and nulls:
            return parts[0], True
        if len(parts) > 1 and nulls:
            return {"anyOf": parts}, True
    types = node.get("type")
    if isinstance(types, list) and "null" in types:
        non = [t for t in types if t != "null"]
        inner = {k: v for k, v in node.items() if k != "type"}
        inner["type"] = non[0] if len(non) == 1 else non
        return inner, True
    return node, False


def _disc(alt: dict[str, Any]) -> tuple[str, str] | None:
    props = alt.get("properties") or {}
    for key in ("kind", "type"):
        val = _single_str_enum(props.get(key))
        if val is not None:
            return key, val
    return None


def _object_disc_value(node: dict[str, Any], key: str) -> str | None:
    props = node.get("properties") or {}
    if key not in props:
        return None
    p = props[key]
    return _single_str_enum(p) or (
        str(p["default"]) if isinstance(p.get("default"), str) else None
    )


def _root_body(schema: dict[str, Any]) -> dict[str, Any]:
    return {
        k: v
        for k, v in schema.items()
        if k not in ("definitions", "$defs", "$schema")
    }


@dataclass
class Emitter:
    schema: dict[str, Any]
    module_doc: str
    emitted: set[str] = field(default_factory=set)
    blocks: list[str] = field(default_factory=list)
    needs_any: bool = False

    def defs(self) -> dict[str, Any]:
        return _defs(self.schema)

    def find_def_for_disc(self, key: str, value: str) -> str | None:
        for name, node in self.defs().items():
            if node.get("type") == "object" and _object_disc_value(node, key) == value:
                return name
        return None

    def variant_name(self, parent: str, alt: dict[str, Any], index: int) -> str:
        if (r := _ref(alt)):
            return r
        disc = _disc(alt)
        if not disc:
            return f"{parent}{index + 1}"
        key, val = disc
        if (existing := self.find_def_for_disc(key, val)):
            return existing
        if key == "kind":
            return f"{_pascal(val)}Prim"
        parts = _camel_parts(parent)
        if parent.endswith("Spec"):
            return f"{_pascal(val)}Node"
        if len(parts) >= 2:
            return _pascal(val) + "".join(parts[1:])
        return _pascal(val)

    def single_enum_value(self, node: dict[str, Any]) -> str | None:
        """Resolve inline / $ref single-value string enums (disc tags)."""
        inner, _ = _unwrap(node)
        if (r := _ref(inner)) and r in self.defs():
            return _single_str_enum(self.defs()[r])
        return _single_str_enum(inner)

    def ty(self, node: dict[str, Any]) -> str:
        node, nullable = _unwrap(node)
        if (r := _ref(node)):
            expr = r
        elif node.get("type") == "string" and "enum" in node:
            expr = f"Literal[{', '.join(repr(v) for v in node['enum'])}]"
        elif "oneOf" in node:
            # Inline oneOf should already be lifted; fall back to union of names.
            expr = " | ".join(
                self.variant_name("Anon", a, i) for i, a in enumerate(node["oneOf"])
            )
        elif "anyOf" in node:
            expr = " | ".join(self.ty(p) for p in node["anyOf"])
        elif node.get("type") == "array":
            items = node.get("items")
            if isinstance(items, list) and len(items) == 2:
                expr = f"tuple[{self.ty(items[0])}, {self.ty(items[1])}]"
            elif isinstance(items, dict):
                expr = f"list[{self.ty(items)}]"
            else:
                self.needs_any = True
                expr = "list[Any]"
        elif node.get("type") == "string":
            expr = "str"
        elif node.get("type") == "integer":
            expr = "int"
        elif node.get("type") == "number":
            expr = "float"
        elif node.get("type") == "boolean":
            expr = "bool"
        elif node.get("type") == "object":
            self.needs_any = True
            expr = "dict[str, Any]"
        else:
            self.needs_any = True
            expr = "Any"
        return f"{expr} | None" if nullable else expr

    def preemit(self, node: dict[str, Any]) -> None:
        node, _ = _unwrap(node)
        if (r := _ref(node)) and r in self.defs():
            self.emit_def(r)
        items = node.get("items")
        if isinstance(items, dict):
            self.preemit(items)
        elif isinstance(items, list):
            for it in items:
                self.preemit(it)
        for key in ("anyOf", "oneOf", "allOf"):
            for part in node.get(key) or []:
                self.preemit(part)

    def emit_def(self, name: str) -> None:
        if name in self.emitted:
            return
        node = self.defs().get(name)
        if not node:
            return
        if node.get("type") == "string" and "enum" in node:
            lit = ", ".join(repr(v) for v in node["enum"])
            self.blocks.append(f"{name} = Literal[{lit}]")
            self.emitted.add(name)
            return
        if "oneOf" in node:
            self.emit_union(name, node)
            return
        if "anyOf" in node:
            for p in node["anyOf"]:
                self.preemit(p)
            expr = " | ".join(self.ty(p) for p in node["anyOf"])
            self.blocks.append(f"{name} = {expr}")
            self.emitted.add(name)
            return
        if node.get("type") == "object" or "properties" in node:
            self.emit_object(name, node)

    def emit_union(self, name: str, node: dict[str, Any]) -> None:
        alts = node["oneOf"]
        # Rule: oneOf of plain string enums → single Literal.
        if all(
            a.get("type") == "string" and "enum" in a and "properties" not in a
            for a in alts
        ):
            vals: list[str] = []
            for a in alts:
                vals.extend(str(v) for v in a["enum"])
            self.blocks.append(f"{name} = Literal[{', '.join(repr(v) for v in vals)}]")
            self.emitted.add(name)
            return

        members: list[str] = []
        disc_key: str | None = None
        for i, alt in enumerate(alts):
            mname = self.variant_name(name, alt, i)
            disc = _disc(alt)
            if disc:
                disc_key = disc[0]
            if mname in self.defs() and mname not in self.emitted:
                self.emit_def(mname)
            elif mname not in self.emitted and "properties" in alt:
                self.emit_object(mname, alt)
                if disc:
                    self._force_literal(mname, disc[0], disc[1])
            members.append(mname)

        self.emitted.add(name)
        if len(members) == 1:
            self.blocks.append(f"{name} = {members[0]}")
        elif disc_key:
            u = " | ".join(members)
            self.blocks.append(
                f"{name} = Annotated[{u}, Field(discriminator={disc_key!r})]"
            )
        else:
            self.blocks.append(f"{name} = {' | '.join(members)}")

    def _force_literal(self, cls: str, key: str, value: str) -> None:
        for i in range(len(self.blocks) - 1, -1, -1):
            if not self.blocks[i].startswith(f"class {cls}("):
                continue
            lines = self.blocks[i].splitlines()
            out = []
            for line in lines:
                if re.match(rf"\s+{key}:", line):
                    out.append(f"    {key}: Literal[{value!r}] = {value!r}")
                else:
                    out.append(line)
            self.blocks[i] = "\n".join(out)
            return

    def emit_object(self, name: str, node: dict[str, Any]) -> None:
        if name in self.emitted:
            return
        self.emitted.add(name)  # break recursion
        props = dict(node.get("properties") or {})
        for pschema in props.values():
            self.preemit(pschema)
        required = set(node.get("required") or [])
        keys = sorted(props, key=lambda k: (k not in required, k))
        for pref in ("type", "kind"):
            if pref in keys:
                keys.remove(pref)
                keys.insert(0, pref)
        fields: list[str] = []
        for pname in keys:
            # Disc tags: single-value string enum → Literal for pydantic tagged unions.
            if pname in ("type", "kind"):
                if (tag := self.single_enum_value(props[pname])) is not None:
                    fields.append(f"    {pname}: Literal[{tag!r}] = {tag!r}")
                    continue
            ann, default, fargs = self._field(props[pname], required=pname in required)
            # Prefer ``= None`` / plain defaults so pyright sees optional ctor params
            # (``Field(None, …)`` is often treated as required by the type checker).
            if default == "None":
                if fargs:
                    ann = f"Annotated[{ann}, Field({', '.join(fargs)})]"
                line = f"    {pname}: {ann} = None"
            elif default is not None and default.startswith("default_factory"):
                if fargs:
                    line = f"    {pname}: {ann} = Field({default}, {', '.join(fargs)})"
                else:
                    line = f"    {pname}: {ann} = Field({default})"
            elif default is not None:
                if fargs:
                    line = f"    {pname}: {ann} = Field(default={default}, {', '.join(fargs)})"
                else:
                    line = f"    {pname}: {ann} = {default}"
            elif fargs:
                line = f"    {pname}: Annotated[{ann}, Field({', '.join(fargs)})]"
            else:
                line = f"    {pname}: {ann}"
            fields.append(line)
        desc = (node.get("description") or "").splitlines()
        doc = f'\n    """{desc[0]}"""\n' if desc else "\n"
        self.blocks.append(
            f"class {name}(StrictModel):{doc}" + ("\n".join(fields) if fields else "    pass")
        )

    def _field(
        self, pschema: dict[str, Any], *, required: bool
    ) -> tuple[str, str | None, list[str]]:
        fargs: list[str] = []
        if desc := pschema.get("description"):
            fargs.append(f"description={desc.splitlines()[0]!r}")
        inner, nullable = _unwrap(pschema)
        ann = self.ty(pschema)
        has_default = "default" in pschema
        default_val = pschema.get("default") if has_default else None
        is_array = inner.get("type") == "array" or (
            isinstance(inner.get("type"), list) and "array" in inner["type"]
        )
        default: str | None = None
        if has_default:
            if default_val == []:
                default = "default_factory=list"
            elif default_val == {}:
                default = "default_factory=dict"
            else:
                default = repr(default_val)
        elif not required and is_array and not nullable:
            default = "default_factory=list"
            if ann.endswith(" | None"):
                ann = ann[: -len(" | None")]
        elif not required or nullable:
            if not ann.endswith(" | None"):
                ann = f"{ann} | None"
            default = "None"
        if has_default and default_val == [] and "list[" in ann:
            default = "default_factory=list"
            if ann.endswith(" | None"):
                ann = ann[: -len(" | None")]
        return ann, default, fargs

    def emit_root(self, title: str, root: dict[str, Any]) -> None:
        if "oneOf" in root:
            self.emit_union(title, root)
            members = self._union_members(title)
            if members and _disc(root["oneOf"][0]):
                # Replace plain Annotated alias with RootModel.
                self.blocks = [b for b in self.blocks if not b.startswith(f"{title} = ")]
                disc = _disc(root["oneOf"][0])
                assert disc
                u = " | ".join(members)
                self.blocks.append(
                    f"{title}Root = Annotated[{u}, Field(discriminator={disc[0]!r})]\n\n"
                    f"class {title}(RootModel[{title}Root]):\n"
                    f'    """{root.get("description", title).splitlines()[0]}"""\n\n'
                    f"    def model_dump(self, *args, **kwargs):\n"
                    f"        kwargs.setdefault('exclude_none', True)\n"
                    f"        return super().model_dump(*args, **kwargs)\n\n"
                    f"    def model_dump_json(self, *args, **kwargs):\n"
                    f"        kwargs.setdefault('exclude_none', True)\n"
                    f"        return super().model_dump_json(*args, **kwargs)\n"
                )
        elif root.get("type") == "object" or "properties" in root:
            self.emit_object(title, root)
        else:
            raise ValueError(f"unsupported root schema for {title!r}")

    def emit_remaining_defs(self) -> None:
        for name in list(self.defs()):
            self.emit_def(name)

    def _union_members(self, name: str) -> list[str]:
        for b in self.blocks:
            if b.startswith(f"{name} = Annotated["):
                inner = b[len(f"{name} = Annotated[") :].split(", Field")[0]
                return [p.strip() for p in inner.split("|")]
            if b.startswith(f"{name} = ") and "|" in b and "Annotated" not in b:
                return [p.strip() for p in b.split("=", 1)[1].split("|")]
            if b.startswith(f"{name} = ") and "Annotated" not in b and "|" not in b:
                return [b.split("=", 1)[1].strip()]
        return []

    def render(self) -> str:
        body = "\n\n\n".join(self.blocks)
        typing_imports = ["Literal"]
        if "Annotated[" in body:
            typing_imports.insert(0, "Annotated")
        if self.needs_any or re.search(r"\bAny\b", body):
            typing_imports.append("Any")
        pydantic_imports = ["BaseModel", "ConfigDict", "Field"]
        if "RootModel" in body:
            pydantic_imports.append("RootModel")
        return (
            "# Auto-generated from Rust schemars (make types) — do not edit.\n"
            f'"""{self.module_doc}"""\n\n'
            "from __future__ import annotations\n\n"
            f"from typing import {', '.join(typing_imports)}\n\n"
            f"from pydantic import {', '.join(pydantic_imports)}\n\n\n"
            f"{STRICT_MODEL}\n"
            f"{body}\n"
        )


def emit_module(schema_files: tuple[str, ...], module_doc: str) -> str:
    """Load schema file(s), merge definitions, emit each title as a root."""
    schemas = [_load(name) for name in schema_files]
    for s, name in zip(schemas, schema_files):
        if not s.get("title"):
            raise ValueError(f"{name}: schema root needs a title")
    merged_defs: dict[str, Any] = {}
    for s in schemas:
        merged_defs.update(_defs(s))
    em = Emitter({"definitions": merged_defs}, module_doc)
    for s in schemas:
        title = str(s["title"])
        em.emit_root(title, _root_body(s))
    em.emit_remaining_defs()
    return em.render()


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for filename, schema_files, doc in MODULES:
        text = emit_module(schema_files, doc)
        path = OUT / filename
        path.write_text(text, encoding="utf-8")
        print(f"wrote {path.relative_to(ROOT)} ({len(text.splitlines())} lines)")


if __name__ == "__main__":
    main()
