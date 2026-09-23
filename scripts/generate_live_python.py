#!/usr/bin/env python3
"""Emit live Pydantic StrictModels from schemars JSON Schema (Rust SoT).

Reads ``schema/{edge-plan,edge-result,scene,xpict}.schema.json`` and writes
``python/xpict/contracts/{edge,scene,depict}.py``. Prefer this over
datamodel-codegen (which invents Primitive1 / DepictSpec2 names).

Invoked by ``scripts/generate_live_types.sh`` / ``make types``.
"""

from __future__ import annotations

import json
import re
import textwrap
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = ROOT / "schema"
OUT = ROOT / "python" / "xpict" / "contracts"

HEADER = '''\
# Auto-generated from Rust schemars (make types) — do not edit.
"""{doc}"""

from __future__ import annotations

from typing import Annotated, Literal{extra_typing}

from pydantic import BaseModel, ConfigDict, Field, RootModel


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")

    def model_dump(self, *args, **kwargs):
        # Match Rust ``skip_serializing_if = Option::is_none`` / empty skips.
        kwargs.setdefault("exclude_none", True)
        return super().model_dump(*args, **kwargs)

    def model_dump_json(self, *args, **kwargs):
        kwargs.setdefault("exclude_none", True)
        return super().model_dump_json(*args, **kwargs)
'''

# Tagged-union variant names (discriminator value → class name).
KIND_NAMES: dict[str, str] = {
    "path": "PathPrim",
    "circle": "CirclePrim",
    "text": "TextPrim",
    "coord_gen": "CoordGenTask",  # overridden per-schema when needed
}
TYPE_NAMES: dict[str, str] = {
    "mol": "MolNode",
    "group": "GroupNode",
    "coord_gen": "CoordGenTask",
}


def _load(name: str) -> dict[str, Any]:
    return json.loads((SCHEMA / name).read_text(encoding="utf-8"))


def _defs(schema: dict[str, Any]) -> dict[str, Any]:
    return dict(schema.get("definitions") or schema.get("$defs") or {})


def _single_enum(node: dict[str, Any] | None) -> str | None:
    if not node:
        return None
    enum = node.get("enum")
    if isinstance(enum, list) and len(enum) == 1 and isinstance(enum[0], str):
        return enum[0]
    if node.get("const") is not None and isinstance(node["const"], str):
        return str(node["const"])
    return None


def _is_null(node: dict[str, Any]) -> bool:
    return node.get("type") == "null"


def _unwrap_null(node: dict[str, Any]) -> tuple[dict[str, Any], bool]:
    """Return (inner, nullable)."""
    # schemars often wraps $ref defaults as allOf: [{$ref}]
    if "allOf" in node and len(node["allOf"]) == 1 and isinstance(node["allOf"][0], dict):
        merged = {k: v for k, v in node.items() if k != "allOf"}
        merged.update(node["allOf"][0])
        return _unwrap_null(merged)
    if "anyOf" in node:
        parts = [p for p in node["anyOf"] if not _is_null(p)]
        nulls = [p for p in node["anyOf"] if _is_null(p)]
        if len(parts) == 1 and nulls:
            return parts[0], True
        if len(parts) > 1 and nulls:
            return {"anyOf": parts}, True
    types = node.get("type")
    if isinstance(types, list) and "null" in types:
        non_null = [t for t in types if t != "null"]
        inner = {k: v for k, v in node.items() if k != "type"}
        if len(non_null) == 1:
            inner["type"] = non_null[0]
        else:
            inner["type"] = non_null
        return inner, True
    return node, False


def _ref_name(node: dict[str, Any]) -> str | None:
    ref = node.get("$ref")
    if not isinstance(ref, str):
        return None
    return ref.rsplit("/", 1)[-1]


def _pascal_from_snake(s: str) -> str:
    return "".join(p.capitalize() for p in s.split("_"))


@dataclass
class Emitter:
    schema: dict[str, Any]
    module_doc: str
    # name overrides for oneOf variants: (parent_name, disc_value) -> class name
    variant_names: dict[tuple[str, str], str] = field(default_factory=dict)
    # union aliases to emit after classes: name -> list of member class names
    unions: dict[str, list[str]] = field(default_factory=dict)
    # RootModel wrappers: name -> (union_type_expr, extra_methods)
    roots: dict[str, tuple[str, str]] = field(default_factory=dict)
    # Literal type aliases from string enums
    literals: dict[str, list[str]] = field(default_factory=dict)
    # Classes already emitted
    emitted: set[str] = field(default_factory=set)
    class_blocks: list[str] = field(default_factory=list)
    needs_any: bool = False
    # When emitting DepictSpec, skip duplicate MolNode from oneOf mol branch
    skip_defs: set[str] = field(default_factory=set)

    def defs(self) -> dict[str, Any]:
        return _defs(self.schema)

    def type_expr(self, node: dict[str, Any], *, prop: str | None = None) -> str:
        node, nullable = _unwrap_null(node)
        ref = _ref_name(node)
        if ref:
            base = ref
            # Prefer literal alias if we emitted one for an enum def
            if ref in self.literals:
                base = ref
            expr = base
        elif "enum" in node and node.get("type", "string") in ("string", None):
            vals = node["enum"]
            expr = " | ".join(repr(v) for v in vals)
            expr = f"Literal[{', '.join(repr(v) for v in vals)}]" if vals else "str"
        elif "oneOf" in node:
            members = []
            for i, alt in enumerate(node["oneOf"]):
                members.append(self._oneof_member_name(prop or "Variant", alt, i))
            expr = " | ".join(members)
        elif "anyOf" in node:
            parts = [self.type_expr(p) for p in node["anyOf"]]
            expr = " | ".join(parts)
        elif node.get("type") == "array":
            items = node.get("items")
            if isinstance(items, list) and len(items) == 2:
                # tuple pair
                a = self.type_expr(items[0])
                b = self.type_expr(items[1])
                expr = f"tuple[{a}, {b}]"
            elif isinstance(items, dict):
                expr = f"list[{self.type_expr(items)}]"
            else:
                self.needs_any = True
                expr = "list[Any]"
        elif node.get("type") == "object" and "properties" in node:
            # anonymous object — should have been lifted to a class
            self.needs_any = True
            expr = "dict[str, Any]"
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
        if nullable:
            return f"{expr} | None"
        return expr

    def _disc_field(self, alt: dict[str, Any]) -> tuple[str, str] | None:
        props = alt.get("properties") or {}
        for key in ("kind", "type"):
            if key in props:
                val = _single_enum(props[key])
                if val is not None:
                    return key, val
        return None

    def _oneof_member_name(self, parent: str, alt: dict[str, Any], index: int) -> str:
        ref = _ref_name(alt)
        if ref:
            return ref
        disc = self._disc_field(alt)
        if disc:
            key, val = disc
            override = self.variant_names.get((parent, val))
            if override:
                return override
            if key == "kind" and val in KIND_NAMES:
                return KIND_NAMES[val]
            if key == "type" and val in TYPE_NAMES:
                return TYPE_NAMES[val]
            return _pascal_from_snake(val)
        return f"{parent}{index + 1}"

    def ensure_enum_literal(self, name: str, node: dict[str, Any]) -> None:
        if name in self.emitted:
            return
        if node.get("type") == "string" and "enum" in node:
            vals = [str(v) for v in node["enum"]]
            self.literals[name] = vals
            lit = ", ".join(repr(v) for v in vals)
            desc = node.get("description")
            block = f"{name} = Literal[{lit}]"
            if desc:
                block = f"# {desc.splitlines()[0]}\n{block}"
            self.class_blocks.append(block)
            self.emitted.add(name)

    def emit_object(self, name: str, node: dict[str, Any]) -> None:
        if name in self.emitted or name in self.skip_defs:
            return
        # Break recursive structs (e.g. MolTemplate.template_for).
        self.emitted.add(name)
        # first emit dependencies
        props: dict[str, Any] = dict(node.get("properties") or {})
        for pname, pschema in props.items():
            self._preemit_deps(pschema)
        required = set(node.get("required") or [])
        fields: list[str] = []
        # stable-ish order: required first, then alpha
        keys = sorted(props.keys(), key=lambda k: (k not in required, k))
        # Prefer discriminator fields first for readability
        for pref in ("type", "kind"):
            if pref in keys:
                keys.remove(pref)
                keys.insert(0, pref)
        for pname in keys:
            pschema = props[pname]
            ann, default_src, field_args = self._field_bits(pname, pschema, required=pname in required)
            py_name = pname
            if pname == "cls":
                # keep wire name cls (matches Rust rename)
                pass
            line = f"    {py_name}: {ann}"
            extras = []
            if field_args:
                extras.extend(field_args)
            if default_src is not None:
                if extras:
                    line += f" = Field({default_src}, {', '.join(extras)})"
                else:
                    if default_src.startswith("default_factory") or default_src.startswith("default="):
                        line += f" = Field({default_src})"
                    else:
                        line += f" = {default_src}"
            elif extras:
                line += f" = Field({', '.join(extras)})"
            fields.append(line)

        desc = node.get("description")
        doc = f'\n    """{desc.splitlines()[0]}"""\n' if desc else "\n"
        body = "\n".join(fields) if fields else "    pass"
        block = f"class {name}(StrictModel):{doc}{body}"
        self.class_blocks.append(block)

    def _preemit_deps(self, node: dict[str, Any]) -> None:
        node, _ = _unwrap_null(node)
        ref = _ref_name(node)
        if ref and ref in self.defs():
            self.emit_def(ref)
        if "items" in node:
            items = node["items"]
            if isinstance(items, dict):
                self._preemit_deps(items)
            elif isinstance(items, list):
                for it in items:
                    self._preemit_deps(it)
        for key in ("anyOf", "oneOf", "allOf"):
            for part in node.get(key) or []:
                self._preemit_deps(part)
        if node.get("type") == "object" and "properties" in node and "oneOf" not in node:
            # anonymous — ignore
            pass
        if "oneOf" in node:
            # parent context unknown here
            pass

    def _field_bits(
        self, pname: str, pschema: dict[str, Any], *, required: bool
    ) -> tuple[str, str | None, list[str]]:
        field_args: list[str] = []
        desc = pschema.get("description")
        if desc:
            field_args.append(f"description={desc.splitlines()[0]!r}")

        inner, nullable = _unwrap_null(pschema)
        # pair arrays → tuple
        ann = self.type_expr(pschema, prop=pname)

        has_default = "default" in pschema
        default_val = pschema.get("default") if has_default else None

        default_src: str | None = None
        # Bare arrays omitted from `required` → empty list (serde default), not null.
        is_array = inner.get("type") == "array" or (
            isinstance(inner.get("type"), list) and "array" in inner["type"]
        )
        if not required or has_default or nullable:
            if has_default:
                if default_val == [] or default_val == {}:
                    default_src = (
                        "default_factory=list" if default_val == [] else "default_factory=dict"
                    )
                else:
                    default_src = repr(default_val)
            elif is_array and not nullable:
                default_src = "default_factory=list"
                if ann.endswith(" | None"):
                    ann = ann[: -len(" | None")]
            elif not required or nullable:
                if ann.endswith("| None") or nullable:
                    if not ann.endswith("| None"):
                        ann = f"{ann} | None"
                    default_src = "None"
                else:
                    default_src = "None"
                    ann = f"{ann} | None"

        if has_default and default_val == [] and "list[" in ann:
            default_src = "default_factory=list"
            if ann.endswith(" | None"):
                ann = ann[: -len(" | None")]

        return ann, default_src, field_args

    def emit_def(self, name: str) -> None:
        if name in self.emitted or name in self.skip_defs:
            return
        node = self.defs().get(name)
        if not node:
            return
        if node.get("type") == "string" and "enum" in node:
            self.ensure_enum_literal(name, node)
            return
        if "oneOf" in node:
            self.emit_tagged_union(name, node)
            return
        if "anyOf" in node and all(_ref_name(p) or p.get("type") == "string" for p in node["anyOf"]):
            # untagged union type alias (e.g. AlignTo)
            parts = []
            for p in node["anyOf"]:
                self._preemit_deps(p)
                parts.append(self.type_expr(p))
            expr = " | ".join(parts)
            desc = node.get("description")
            block = f"{name} = {expr}"
            if desc:
                block = f"# {desc.splitlines()[0]}\n{block}"
            self.class_blocks.append(block)
            self.emitted.add(name)
            return
        if node.get("type") == "object" or "properties" in node:
            self.emit_object(name, node)
            return
        # fallback
        self.emitted.add(name)

    def emit_tagged_union(self, name: str, node: dict[str, Any]) -> None:
        # schemars sometimes splits string enums with docs into oneOf of enums —
        # merge into a single Literal alias (e.g. CoordMethod).
        if all(
            isinstance(alt, dict)
            and alt.get("type") == "string"
            and "enum" in alt
            and "properties" not in alt
            for alt in node.get("oneOf") or []
        ):
            vals: list[str] = []
            for alt in node["oneOf"]:
                vals.extend(str(v) for v in alt["enum"])
            self.literals[name] = vals
            lit = ", ".join(repr(v) for v in vals)
            desc = node.get("description")
            block = f"{name} = Literal[{lit}]"
            if desc:
                block = f"# {desc.splitlines()[0]}\n{block}"
            self.class_blocks.append(block)
            self.emitted.add(name)
            return

        members: list[str] = []
        disc_key: str | None = None
        for i, alt in enumerate(node["oneOf"]):
            mname = self._oneof_member_name(name, alt, i)
            disc = self._disc_field(alt)
            if disc:
                disc_key = disc[0]
            if name == "EdgeTaskResult" and disc and disc[1] == "coord_gen":
                mname = "CoordGenTaskResult"
            if name == "EdgeTask" and disc and disc[1] == "coord_gen":
                mname = "CoordGenTask"
            if name == "DepictSpec" and disc and disc[1] == "mol":
                self.emit_def("MolNode")
                members.append("MolNode")
                continue
            if name == "DepictSpec" and disc and disc[1] == "group":
                mname = "GroupNode"
            self.emit_object(mname, alt)
            if disc:
                self._rewrite_disc_literal(mname, disc[0], disc[1])
            members.append(mname)
        self.unions[name] = members
        if len(members) == 1:
            # Single variant — plain alias (avoid Annotated discriminator noise).
            self.class_blocks.append(f"{name} = {members[0]}")
        elif disc_key:
            union_expr = " | ".join(members)
            self.class_blocks.append(
                f"{name} = Annotated[{union_expr}, Field(discriminator={disc_key!r})]"
            )
        else:
            self.class_blocks.append(f"{name} = {' | '.join(members)}")
        self.emitted.add(name)

    def _rewrite_disc_literal(self, class_name: str, key: str, value: str) -> None:
        """Patch the last emitted class so ``type``/``kind`` is Literal['value'] = 'value'."""
        for i in range(len(self.class_blocks) - 1, -1, -1):
            block = self.class_blocks[i]
            if not block.startswith(f"class {class_name}("):
                continue
            # replace the discriminator field line
            lines = block.splitlines()
            new_lines = []
            for line in lines:
                if re.match(rf"\s+{key}:", line):
                    new_lines.append(f"    {key}: Literal[{value!r}] = {value!r}")
                else:
                    new_lines.append(line)
            self.class_blocks[i] = "\n".join(new_lines)
            return

    def emit_root(self, title: str) -> None:
        """Emit the root schema type (may be object or oneOf)."""
        node = {k: v for k, v in self.schema.items() if k not in ("definitions", "$defs", "$schema")}
        if "oneOf" in node:
            self.emit_tagged_union(title, node)
            # Wrap DepictSpec as RootModel
            if title == "DepictSpec":
                members = self.unions.get(title, [])
                union = " | ".join(members)
                methods = textwrap.dedent(
                    '''
                    def model_dump(self, *args, **kwargs):
                        kwargs.setdefault("exclude_none", True)
                        return super().model_dump(*args, **kwargs)

                    def model_dump_json(self, *args, **kwargs):
                        kwargs.setdefault("exclude_none", True)
                        return super().model_dump_json(*args, **kwargs)

                    def mols(self) -> list[MolNode]:
                        root = self.root
                        if isinstance(root, MolNode):
                            return [root]
                        return list(root.children)
                    '''
                ).strip("\n")
                methods = textwrap.indent(methods + "\n", "    ")
                # Replace Annotated union with RootModel
                self.class_blocks = [
                    b
                    for b in self.class_blocks
                    if not b.startswith(f"{title} = Annotated[")
                ]
                self.class_blocks.append(
                    f"DepictRoot = Annotated[{union}, Field(discriminator='type')]\n\n"
                    f"class DepictSpec(RootModel[DepictRoot]):\n"
                    f'    """Declarative document (strict subset of future PictSpec)."""\n'
                    f"{methods}"
                )
                self.class_blocks.append("MolSpec = MolNode")
        elif node.get("type") == "object":
            self.emit_object(title, node)
        # emit all remaining defs referenced
        for name in list(self.defs()):
            self.emit_def(name)

    def render(self) -> str:
        extra = ", Any" if self.needs_any else ""
        # Also need Any if used — detect from blocks
        body = "\n\n\n".join(self.class_blocks)
        if "Any" in body:
            extra = ", Any"
        head = HEADER.format(doc=self.module_doc, extra_typing=extra)
        return head + "\n\n" + body + "\n"


def emit_scene() -> str:
    schema = _load("scene.schema.json")
    em = Emitter(schema, module_doc="Drawable scene graph — live ABI from xpict-core (schemars).")
    # Emit supporting defs first
    for name in ("TextAnchor", "LayerName", "Primitive", "Layer", "Viewport"):
        if name in em.defs() or name == "Primitive":
            if name == "Primitive":
                em.emit_tagged_union("Primitive", em.defs()["Primitive"])
            else:
                em.emit_def(name)
    em.emit_object("Scene", {k: v for k, v in schema.items() if k not in ("definitions", "$defs", "$schema")})
    # Aliases for hand-era names already covered by KIND_NAMES
    return em.render()


def emit_edge() -> str:
    plan = _load("edge-plan.schema.json")
    result = _load("edge-result.schema.json")
    # Merge defs: plan + result (MoleculeIn lives on result)
    merged = {
        "$schema": plan["$schema"],
        "title": "EdgePlan",
        "description": plan.get("description", "EdgePlan / EdgeResult live ABI."),
        "type": "object",
        "required": plan.get("required"),
        "properties": plan.get("properties"),
        "definitions": {},
    }
    merged["definitions"].update(_defs(result))
    merged["definitions"].update(_defs(plan))

    em = Emitter(
        merged,
        module_doc="EdgePlan / EdgeResult — live ABI from xpict-core (schemars).",
    )
    # Order: leaf inputs → plan → result
    for name in (
        "AlignOpts",
        "AtomIn",
        "BondIn",
        "MoleculeIn",
        "MolTemplate",
        "CoordMethod",
        "EdgeTask",
        "EdgePlan",
        "CoordGenMoleculeResult",
        "EdgeTaskResult",
        "EdgeResult",
    ):
        if name == "EdgePlan":
            em.emit_object("EdgePlan", {k: v for k, v in plan.items() if k not in ("definitions", "$defs", "$schema")})
            continue
        if name == "EdgeResult":
            em.emit_object(
                "EdgeResult",
                {k: v for k, v in result.items() if k not in ("definitions", "$defs", "$schema")},
            )
            continue
        if name in em.defs():
            em.emit_def(name)
    # Single-variant EdgeTask already aliased inside emit_tagged_union.
    return em.render()


def emit_depict() -> str:
    schema = _load("xpict.schema.json")
    em = Emitter(
        schema,
        module_doc="DepictSpec — live document ABI from xpict-core (schemars).",
    )
    # Prefer named defs before root
    for name in ("AlignToSpec", "AlignTo", "ShadeSpec", "MolNode", "MolNodeKind"):
        if name in em.defs():
            em.emit_def(name)
    # Discriminator needs Literal on both arms — rewrite MolNode.type.
    em._rewrite_disc_literal("MolNode", "type", "mol")
    em.emit_root("DepictSpec")
    return em.render()


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    mapping = {
        "scene.py": emit_scene,
        "edge.py": emit_edge,
        "depict.py": emit_depict,
    }
    for filename, fn in mapping.items():
        text = fn()
        path = OUT / filename
        path.write_text(text, encoding="utf-8")
        print(f"wrote {path.relative_to(ROOT)} ({len(text.splitlines())} lines)")


if __name__ == "__main__":
    main()
