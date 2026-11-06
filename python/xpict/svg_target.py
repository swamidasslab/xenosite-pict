"""SVG medium defaults for host export formats.

Rust paints ``svg: web`` (feMorphology) by default. PNG/PDF pipelines often
drop SVG filters, so hosts inject ``svg: print`` (path halos) when the author
did not set ``svg``. Notebook / inline SVG stays web.
"""

from __future__ import annotations

from typing import Any, Mapping

# Host output formats that should prefer print-safe SVG when ``svg`` is unset.
PRINT_SVG_FORMATS = frozenset({"png", "pdf"})


def apply_output_svg_default(
    live: Mapping[str, Any],
    format: str,
) -> dict[str, Any]:
    """Return a DepictSpec dict with ``svg: print`` when exporting PNG/PDF.

    No-op when ``format`` is not png/pdf, or when the root already sets ``svg``.
    Does not invent a second paint engine — only stamps the DepictSpec field
    so Rust chooses path halos.
    """
    out = dict(live)
    if format not in PRINT_SVG_FORMATS:
        return out
    if out.get("svg") is not None:
        return out
    out["svg"] = "print"
    return out
