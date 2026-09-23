# Bundled fonts

- **LiberationSans-Regular.ttf**
- **LiberationSans-Bold.ttf**
- **LiberationSans-Italic.ttf**
- **LiberationSans-BoldItalic.ttf**

SIL Open Font License 1.1 (see `LICENSE-LiberationSans.txt`). Metric-compatible
with Arial; good coverage of Latin scientific symbols and Greek used in labels.

Drawn label ink is glyph outlines (Rust `ttf-parser` → SVG paths). Bold/italic
markup selects the matching face. Halos are Rust Shape buffers of the same outlines.

Keep in sync with `python/xpict/data/fonts/` (same files for the Python package).
