"""Shorthand expand/compress and light tree maps for Pict JSON.

Convention used across the schema:

- A bare string is the value with all defaults.
- ``{"text": "..."}`` is equivalent to that string.
- Extra keys override defaults; omitted keys keep defaults.

The library validates/expands at the API boundary and then works only on
fully expanded models.
"""

from __future__ import annotations

from collections.abc import Callable, Mapping, MutableMapping, Sequence
from copy import deepcopy
from typing import Any, TypeVar

T = TypeVar("T")


def map_tree(
    value: Any,
    fn: Callable[[Any, tuple[Any, ...]], Any],
    *,
    path: tuple[Any, ...] = (),
) -> Any:
    """Map ``fn`` over every node of a JSON-like tree (pre-order replace).

    ``fn(node, path)`` returns the replacement for ``node``. Containers are
    remapped first at the node itself, then (if still a container) walked.
    """
    replaced = fn(value, path)
    if isinstance(replaced, Mapping):
        return {
            k: map_tree(v, fn, path=(*path, k)) for k, v in replaced.items()
        }
    if isinstance(replaced, list):
        return [map_tree(v, fn, path=(*path, i)) for i, v in enumerate(replaced)]
    if isinstance(replaced, tuple):
        return tuple(map_tree(v, fn, path=(*path, i)) for i, v in enumerate(replaced))
    return replaced


def map_dicts(
    value: Any,
    fn: Callable[[MutableMapping[str, Any], tuple[Any, ...]], Mapping[str, Any] | None],
    *,
    path: tuple[Any, ...] = (),
) -> Any:
    """Walk a tree and rewrite each ``dict`` via ``fn``.

    ``fn(d, path)`` may return a new mapping, mutate ``d`` and return it, or
    return ``None`` to leave ``d`` (after recursively rewriting children).
    """
    if isinstance(value, Mapping):
        # Recurse into children first so nested shorthands expand bottom-up.
        child = {k: map_dicts(v, fn, path=(*path, k)) for k, v in value.items()}
        out = fn(dict(child), path)
        return child if out is None else out
    if isinstance(value, list):
        return [map_dicts(v, fn, path=(*path, i)) for i, v in enumerate(value)]
    if isinstance(value, tuple):
        return tuple(map_dicts(v, fn, path=(*path, i)) for i, v in enumerate(value))
    return value


def expand_shorthand(
    value: Any,
    defaults: Mapping[str, Any],
    *,
    text_key: str = "text",
) -> dict[str, Any] | None:
    """Expand a string/partial-dict shorthand into a full defaults overlay.

    ``None`` stays ``None``. A string becomes ``{text_key: value, **defaults}``.
    A dict is shallow-merged onto ``defaults`` (and must supply ``text_key``
    unless defaults already include it).
    """
    if value is None:
        return None
    if isinstance(value, str):
        out = dict(defaults)
        out[text_key] = value
        return out
    if isinstance(value, Mapping):
        if text_key not in value and text_key not in defaults:
            raise ValueError(f"shorthand object needs {text_key!r}")
        out = dict(defaults)
        out.update(dict(value))
        return out
    raise TypeError(f"expected str, dict, or None; got {type(value).__name__}")


def compress_shorthand(
    value: Any,
    defaults: Mapping[str, Any],
    *,
    text_key: str = "text",
) -> Any:
    """Drop keys that still match ``defaults``; bare ``text`` collapses to a string.

    ``None`` stays ``None``. Non-dicts are returned unchanged.
    """
    if value is None:
        return None
    if not isinstance(value, Mapping):
        return value
    data = dict(value)
    for key, default in defaults.items():
        if key in data and data[key] == default:
            del data[key]
    if set(data.keys()) == {text_key}:
        return data[text_key]
    return data


def deep_merge(base: Mapping[str, Any], overlay: Mapping[str, Any]) -> dict[str, Any]:
    """Shallow-prefer overlay; nested dicts merge recursively."""
    out = deepcopy(dict(base))
    for key, val in overlay.items():
        if (
            key in out
            and isinstance(out[key], Mapping)
            and isinstance(val, Mapping)
        ):
            out[key] = deep_merge(out[key], val)
        else:
            out[key] = deepcopy(val)
    return out


# --- PictSpec-shaped shorthand (molecule labels, …) -------------------------

# Defaults for MoleculeSpec.label once expanded. ``text`` is required input.
LABEL_DEFAULTS: dict[str, Any] = {
    "pos": "bottom",
}


def expand_label(value: Any) -> dict[str, Any] | None:
    """Expand a molecule-label shorthand to a full label dict."""
    return expand_shorthand(value, LABEL_DEFAULTS)


def compress_label(value: Any) -> Any:
    """Compress an expanded molecule label back to shorthand."""
    return compress_shorthand(value, LABEL_DEFAULTS)


def _alias_molecule_title(mol: MutableMapping[str, Any]) -> dict[str, Any]:
    """Accept legacy ``title`` as an alias for ``label``."""
    out = dict(mol)
    if "title" in out:
        if "label" not in out or out["label"] is None:
            out["label"] = out["title"]
        del out["title"]
    return out


def expand_pict_input(value: Any) -> Any:
    """Fully expand shorthand fields in a PictSpec-shaped JSON tree.

    - ``molecules[*].title`` → ``label``
    - ``molecules[*].label`` string / partial dict → full defaults overlay
    """
    if not isinstance(value, Mapping):
        return value
    root = dict(value)
    mols = root.get("molecules")
    if isinstance(mols, Sequence) and not isinstance(mols, (str, bytes)):
        expanded_mols: list[Any] = []
        for mol in mols:
            if not isinstance(mol, Mapping):
                expanded_mols.append(mol)
                continue
            m = _alias_molecule_title(dict(mol))
            if "label" in m:
                m["label"] = expand_label(m["label"])
            expanded_mols.append(m)
        root["molecules"] = expanded_mols
    return root


def compress_pict_input(value: Any) -> Any:
    """Compress expanded shorthand fields (drop keys that match defaults)."""
    if not isinstance(value, Mapping):
        return value
    root = dict(value)
    mols = root.get("molecules")
    if isinstance(mols, Sequence) and not isinstance(mols, (str, bytes)):
        compressed: list[Any] = []
        for mol in mols:
            if not isinstance(mol, Mapping):
                compressed.append(mol)
                continue
            m = dict(mol)
            if "label" in m:
                m["label"] = compress_label(m["label"])
            compressed.append(m)
        root["molecules"] = compressed
    return root
