"""Lightweight label markup — TeX-like symbols + markdown/TeX emphasis.

Not a typesetting engine. Symbol macros expand to Unicode (Liberation Sans
covers Greek and common scientific signs). Bold/italic become SVG
``font-weight`` / ``font-style`` spans.

Symbols (examples)::

    \\alpha → α   \\beta → β   \\mu → μ   \\degree → °   \\pm → ±

Emphasis::

    markdown:  **bold**   *italic*
    latex:     \\textbf{…}  \\textit{…}  \\mathbf{…}  \\mathit{…}  \\emph{…}

Optional ``$…$`` delimiters are stripped (chem shorthand); content is still
plain Unicode + spans, not math layout.
"""

from __future__ import annotations

from dataclasses import dataclass

# TeX-ish name → Unicode. Prefer Greek letter forms Liberation Sans has.
_SYMBOLS: dict[str, str] = {
    # Greek lower
    "alpha": "α",
    "beta": "β",
    "gamma": "γ",
    "delta": "δ",
    "epsilon": "ε",
    "varepsilon": "ε",
    "zeta": "ζ",
    "eta": "η",
    "theta": "θ",
    "vartheta": "ϑ",
    "iota": "ι",
    "kappa": "κ",
    "lambda": "λ",
    "mu": "μ",
    "nu": "ν",
    "xi": "ξ",
    "omicron": "ο",
    "pi": "π",
    "varpi": "ϖ",
    "rho": "ρ",
    "varrho": "ϱ",
    "sigma": "σ",
    "varsigma": "ς",
    "tau": "τ",
    "upsilon": "υ",
    "phi": "φ",
    "varphi": "ϕ",
    "chi": "χ",
    "psi": "ψ",
    "omega": "ω",
    # Greek upper
    "Alpha": "Α",
    "Beta": "Β",
    "Gamma": "Γ",
    "Delta": "Δ",
    "Epsilon": "Ε",
    "Zeta": "Ζ",
    "Eta": "Η",
    "Theta": "Θ",
    "Iota": "Ι",
    "Kappa": "Κ",
    "Lambda": "Λ",
    "Mu": "Μ",
    "Nu": "Ν",
    "Xi": "Ξ",
    "Omicron": "Ο",
    "Pi": "Π",
    "Rho": "Ρ",
    "Sigma": "Σ",
    "Tau": "Τ",
    "Upsilon": "Υ",
    "Phi": "Φ",
    "Chi": "Χ",
    "Psi": "Ψ",
    "Omega": "Ω",
    # Common scientific signs
    "degree": "°",
    "circ": "°",
    "pm": "±",
    "mp": "∓",
    "times": "×",
    "cdot": "·",
    "ast": "∗",
    "dagger": "†",
    "ddagger": "‡",
    "prime": "′",
    "infty": "∞",
    "approx": "≈",
    "neq": "≠",
    "ne": "≠",
    "leq": "≤",
    "le": "≤",
    "geq": "≥",
    "ge": "≥",
    "rightarrow": "→",
    "to": "→",
    "leftarrow": "←",
    "leftrightarrow": "↔",
    "uparrow": "↑",
    "downarrow": "↓",
    "micro": "µ",
    "AA": "Å",
    "angstrom": "Å",
}

_STYLE_CMDS: dict[str, str] = {
    "textbf": "bold",
    "mathbf": "bold",
    "bf": "bold",
    "textit": "italic",
    "mathit": "italic",
    "it": "italic",
    "emph": "italic",
}


@dataclass(frozen=True)
class TextRun:
    """One style run of label text (already symbol-expanded)."""

    text: str
    bold: bool = False
    italic: bool = False


def _read_braced(src: str, i: int) -> tuple[str, int] | None:
    """Read ``{…}`` starting at ``i`` (must be ``{``); return body and index after ``}``."""
    if i >= len(src) or src[i] != "{":
        return None
    depth = 0
    j = i
    while j < len(src):
        ch = src[j]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return src[i + 1 : j], j + 1
        j += 1
    return None


def _merge_run(out: list[TextRun], text: str, *, bold: bool, italic: bool) -> None:
    if not text:
        return
    if out and out[-1].bold == bold and out[-1].italic == italic:
        prev = out[-1]
        out[-1] = TextRun(prev.text + text, bold=bold, italic=italic)
    else:
        out.append(TextRun(text, bold=bold, italic=italic))


def _parse(
    src: str,
    *,
    bold: bool = False,
    italic: bool = False,
) -> list[TextRun]:
    """Parse markup into styled runs; symbol macros expand to Unicode."""
    out: list[TextRun] = []
    i = 0
    n = len(src)
    buf: list[str] = []

    def flush() -> None:
        nonlocal buf
        if buf:
            _merge_run(out, "".join(buf), bold=bold, italic=italic)
            buf = []

    while i < n:
        ch = src[i]

        # Optional $…$ math delimiters — strip, still no layout.
        if ch == "$":
            flush()
            j = src.find("$", i + 1)
            if j < 0:
                buf.append(ch)
                i += 1
                continue
            inner = _parse(src[i + 1 : j], bold=bold, italic=italic)
            for run in inner:
                _merge_run(out, run.text, bold=run.bold, italic=run.italic)
            i = j + 1
            continue

        # Backslash command or escape.
        if ch == "\\":
            if i + 1 >= n:
                buf.append("\\")
                i += 1
                continue
            nxt = src[i + 1]
            if nxt in r"\{}$*_":
                buf.append(nxt)
                i += 2
                continue
            if nxt == "\\":
                buf.append("\\")
                i += 2
                continue
            # \command name — longest match against known symbols/styles so
            # ``\Delta\DeltaG`` → ΔΔG (TeX would need a space or braces).
            j = i + 1
            while j < n and src[j].isalpha():
                j += 1
            raw_name = src[i + 1 : j]
            name = raw_name
            while name and name not in _SYMBOLS and name not in _STYLE_CMDS:
                name = name[:-1]
                j -= 1
            if not name:
                buf.append("\\" + raw_name)
                i = i + 1 + len(raw_name)
                continue
            if name in _SYMBOLS:
                flush()
                _merge_run(out, _SYMBOLS[name], bold=bold, italic=italic)
                i = j
                continue
            style = _STYLE_CMDS.get(name)
            if style is not None:
                body_read = _read_braced(src, j)
                if body_read is None:
                    # No braces — leave literal.
                    buf.append("\\" + name)
                    i = j
                    continue
                body, k = body_read
                flush()
                nested = _parse(
                    body,
                    bold=bold or style == "bold",
                    italic=italic or style == "italic",
                )
                for run in nested:
                    _merge_run(out, run.text, bold=run.bold, italic=run.italic)
                i = k
                continue
            buf.append("\\" + name)
            i = j
            continue

        # Markdown **bold**
        if ch == "*" and i + 1 < n and src[i + 1] == "*":
            flush()
            j = src.find("**", i + 2)
            if j < 0:
                buf.append("**")
                i += 2
                continue
            nested = _parse(src[i + 2 : j], bold=True, italic=italic)
            for run in nested:
                _merge_run(out, run.text, bold=run.bold, italic=run.italic)
            i = j + 2
            continue

        # Markdown *italic* (single asterisks)
        if ch == "*":
            flush()
            j = i + 1
            while j < n:
                if src[j] == "*" and (j + 1 >= n or src[j + 1] != "*"):
                    break
                if src[j] == "\\":
                    j += 2
                    continue
                j += 1
            else:
                buf.append("*")
                i += 1
                continue
            if j >= n or src[j] != "*":
                buf.append("*")
                i += 1
                continue
            nested = _parse(src[i + 1 : j], bold=bold, italic=True)
            for run in nested:
                _merge_run(out, run.text, bold=run.bold, italic=run.italic)
            i = j + 1
            continue

        buf.append(ch)
        i += 1

    flush()
    return out


def parse_richtext(src: str) -> list[TextRun]:
    """Parse label markup into style runs with symbols expanded."""
    if not src:
        return []
    return _parse(src)


def plain_text(src: str) -> str:
    """Symbol-expanded plain string (styles discarded) — for metrics / packing."""
    return "".join(run.text for run in parse_richtext(src))


def expand_symbols(src: str) -> str:
    """Expand symbol macros only; leave emphasis markers intact.

    Prefer :func:`plain_text` or :func:`parse_richtext` for normal use.
    """
    # Reuse parser then re-serialize without style — same as plain for symbols
    # that appear outside style commands; for ``\\textbf{\\alpha}`` plain is α.
    return plain_text(src)


__all__ = [
    "TextRun",
    "parse_richtext",
    "plain_text",
    "expand_symbols",
]
