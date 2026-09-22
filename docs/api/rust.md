# Rust API

rustdoc for the paint core and (when system RDKit is available) the native
crate.

<div class="api-frame" markdown="0">
  <iframe
    src="../../rustdoc/xpict_core/index.html"
    title="rustdoc for xpict-core"
    loading="lazy"
  ></iframe>
</div>

| Crate | Docs |
| --- | --- |
| `xpict-core` | [rustdoc](../../rustdoc/xpict_core/index.html) |
| `xpict` | Built when RDKit headers are present; otherwise see [crates.io](https://docs.rs/xpict) |

[Open rustdoc full page](../../rustdoc/xpict_core/index.html){ target=_blank }

Regenerate with `cargo doc -p xpict-core --no-deps` (Pages build does this
automatically into `/rustdoc/`).
