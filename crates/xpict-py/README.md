# xpict-py

PyO3 template for `xpict._native`. **Add new exports in `src/lib.rs`** by wrapping
`xpict-core` functions with `#[pyfunction]` and registering them in `#[pymodule]`.

```bash
maturin develop --manifest-path crates/xpict-py/Cargo.toml
python -c "from xpict import _native; print(_native.multi_bond_offset(20))"
```

Wire Python through `python/xpict/native_bridge.py` (fallback when the extension
is missing). Do not call `_native` from draw code directly except in the bridge.
