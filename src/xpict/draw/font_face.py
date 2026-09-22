"""Bundled Liberation Sans faces — style helpers (outlines live in Rust)."""

from __future__ import annotations

from importlib import resources
from pathlib import Path
from typing import Literal

_FONT_PACKAGE = "xpict.data.fonts"

FaceStyle = Literal["regular", "bold", "italic", "bold_italic"]

_FACE_FILES: dict[FaceStyle, str] = {
    "regular": "LiberationSans-Regular.ttf",
    "bold": "LiberationSans-Bold.ttf",
    "italic": "LiberationSans-Italic.ttf",
    "bold_italic": "LiberationSans-BoldItalic.ttf",
}


def face_style(*, bold: bool = False, italic: bool = False) -> FaceStyle:
    if bold and italic:
        return "bold_italic"
    if bold:
        return "bold"
    if italic:
        return "italic"
    return "regular"


def bundled_font_path(style: FaceStyle = "regular") -> Path:
    """Filesystem path to a packaged Liberation Sans TTF (docs / inspection)."""
    root = resources.files(_FONT_PACKAGE)
    return Path(str(root.joinpath(_FACE_FILES[style])))
