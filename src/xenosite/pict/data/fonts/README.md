# Bundled fonts

- **LiberationSans-Regular.ttf** — SIL Open Font License 1.1 (see
  `LICENSE-LiberationSans.txt`). Metric-compatible with Arial; good coverage
  of Latin scientific symbols (° , µ , dashes, Greek used in labels).

Used at draw time to outline atom labels (fontTools) so the white halo can be
a shapely buffer of the glyph geometry, independent of the viewer’s fonts.
