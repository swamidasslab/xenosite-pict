# Publishing xpict packages

## Version policy

| Axis | Rule |
| --- | --- |
| **major / minor** | **All-or-none** — one `release/vX.Y.0` publishes JS + Python + Rust |
| **major.minor in-tree** | Lockstep across JS, Python, `xpict`, `xpict-core` |
| **patch** | Language-specific — `js/v*`, `py/v*`, `rust-core/v*`, `rust/v*` |

Language tags **cannot** ship `X.Y.0` (rejected by
`scripts/check_version_policy.sh --patch-only`). Cut minors/majors only via
`release/v*`.

| Intent | Tag | Workflow |
| --- | --- | --- |
| Product cut (all surfaces) | `release/v0.2.0` | [`release.yml`](../.github/workflows/release.yml) |
| JS patch only | `js/v0.1.5` (`Z≥1`) | [`publish-js.yml`](../.github/workflows/publish-js.yml) |
| Python patch only | `py/v0.1.5` | [`pypi.yml`](../.github/workflows/pypi.yml) |
| `xpict-core` patch | `rust-core/v0.1.5` | [`crates.yml`](../.github/workflows/crates.yml) |
| Rust `xpict` patch | `rust/v0.1.5` | [`crates.yml`](../.github/workflows/crates.yml) |

CI runs [`scripts/check_version_policy.sh`](../scripts/check_version_policy.sh).

The **public API** to document and version is the single-molecule client:

`mol` → `render(opts?)` → `Rendered` → `toSvg(scene)`  

(not the full nested `PictSpec` / ELK diagram engine).

---

## 0. Major / minor (all-or-none)

1. Bump **every** package to the new `X.Y.0` in one PR:
   `js/package.json`, `pyproject.toml`, `crates/xpict-core`, `crates/xpict`
   (and `xpict-py` / `xpict-wasm` for workspace consistency).
2. `bash scripts/check_version_policy.sh`
3. Merge; CI green.
4. Tag and push:

   ```bash
   git tag release/v0.2.0
   git push origin release/v0.2.0
   ```

5. [`release.yml`](../.github/workflows/release.yml) publishes **xpict-core →
   xpict → JS → Python** at that version (or Actions → **release** workflow
   dispatch).

Do **not** tag `js/v0.2.0` alone — it will fail the patch-only gate.

---

## 1. JavaScript (patch)

```bash
# Z must be >= 1. In-tree major.minor must already match others.
git tag js/v0.1.5
git push origin js/v0.1.5
```

**One-time / org setup (if not already):**

1. Repo lives under the `swamidasslab` GitHub org (scope must match).
2. Workflow permission `packages: write` (already set).
3. Package visibility: `publishConfig.access=public` in `js/package.json` (already set).
4. Consumers need a token that can **read** org packages:

   ```
   @swamidasslab:registry=https://npm.pkg.github.com
   //npm.pkg.github.com/:_authToken=${NPM_TOKEN}
   ```

**Optional — also publish to the public npmjs.org registry** (so no GitHub token is required for install):

1. Create an npmjs.org org / user; claim `@swamidasslab` if available, or change the package name.
2. Add an `NPM_TOKEN` (Automation) secret on this repo.
3. Duplicate or extend the publish job with `registry-url: https://registry.npmjs.org` and `NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}`.
4. Keep GitHub Packages as the xenosite CI source of truth until npmjs is verified.

---

## 2. Rust patches — crates.io

Publish **`xpict-core` first**, then **`xpict`** when both need a patch.

```toml
xpict-core = { path = "../xpict-core", version = "0.1.4" }
```

```bash
git tag rust-core/v0.1.5 && git push origin rust-core/v0.1.5
git tag rust/v0.1.5 && git push origin rust/v0.1.5
```

### One-time setup

1. Create a [crates.io](https://crates.io) account (GitHub login).
2. Confirm crate names are free (`xpict`, `xpict-core`).
3. Add repo secret `CARGO_REGISTRY_TOKEN` (used by `crates.yml` and `release.yml`).
4. Local dry-run:

   ```bash
   cargo publish -p xpict-core --dry-run
   CPLUS_INCLUDE_PATH="$(pwd)/crates/xpict/compat/rdkit" cargo publish -p xpict --dry-run
   ```

`xpict` **requires system RDKit + Boost** at compile time (see `crates/xpict/README.md`).

Do **not** make `xpict-py` / `xpict-wasm` depend on crates.io `xpict` — only `xpict-core`.

---

## 3. Python patch — PyPI

Source under **`python/xpict/`**. Workflow filename **`pypi.yml`**.

```bash
git tag py/v0.1.5
git push origin py/v0.1.5
```

### One-time setup

1. Create a PyPI project `xpict`.
2. Trusted Publishing → this repo, workflow **`pypi.yml`** (and allow `release.yml` if using OIDC there too).
3. Or `PYPI_API_TOKEN` repo secret.

```bash
uv sync --extra rdkit
uv run maturin build --release -m crates/xpict-py/Cargo.toml
```

---

## 4. Public API vs future declarative tree

**Ship / document now:** `mol` / `render` / `toSvg` (+ batch stub
`depict({ molecules: [...] }) → Rendered[]`). Live Pydantic is
`xpict.contracts.depict` (`MolSpec` / `DepictSpec`); JSON Schema is
`schema/xpict.schema.json`.

**Not the publish surface yet:** nested `PictSpec` under `xpict.future` /
`schema/future/`.
