# xenosite-pict

Declarative, publication-quality small-molecule depiction for Python and the web.

Import path: `xenosite.pict`

## Status

Scaffold in progress. Language-neutral JSON contracts (Pydantic → generated JSON Schema) with Python and `js/` engines. Layout backends: Indigo (preferred), RDKit, Chematic (last resort). Multi-molecule diagram layout via ELK. Default output SVG; HTML-with-embedded-SVG for responsive pages.

## Quick intent

```python
from xenosite.pict import Pict, render

svg = Pict(backend="indigo").render({"molecules": [{"smiles": "CCO"}]})
# or
svg = render({"molecules": [{"smiles": "CCO"}]}, backend="indigo")
```

## License

MIT (planned)
