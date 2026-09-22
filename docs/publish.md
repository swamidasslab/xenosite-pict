# Publishing xpict packages

## Version policy

| Axis | Rule |
| --- | --- |
| **major.minor** | Lockstep across JS, Python, and Rust (`xpict` + `xpict-core`) |
| **patch** | Language-specific — tag and publish one surface at a time |

Shared paint / public API changes → bump **minor** (or major) on **every**
package in-tree, then tag each language. Packaging-only or language-edge fixes
→ bump that language’s **patch** only (`js/v0.1.8` while Python stays `0.1.4`
is fine as long as both are `0.1.x`).

CI runs [`scripts/check_version_policy.sh`](../scripts/check_version_policy.sh).
Publish workflows re-check that the tag’s `X.Y` matches the in-tree lockstep line.

| Package | Registry | Tag | Workflow |
| --- | --- | --- | --- |
| `@swamidasslab/xpict` | GitHub Packages (npm) | `js/v*` | [`publish-js.yml`](../.github/workflows/publish-js.yml) |
| `xpict` (Python) | PyPI | `py/v*` | [`pypi.yml`](../.github/workflows/pypi.yml) |
| `xpict-core` | crates.io | `rust-core/v*` | [`crates.yml`](../.github/workflows/crates.yml) |
| `xpict` (Rust) | crates.io | `rust/v*` | [`crates.yml`](../.github/workflows/crates.yml) |

The **public API** to document and version is the single-molecule client:

`mol` → `render(opts?)` → `Rendered` → `toSvg(scene)`  

(not the full nested `PictSpec` / ELK diagram engine).

---

## 1. JavaScript

```bash
# Patch (JS only) — in-tree major.minor must already match others:
# edit js/package.json patch, commit, then:
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

## 2. Rust — crates.io (`xpict-core` then `xpict`)

Publish **`xpict-core` first**, then **`xpict`**. Workspace path deps carry a
`version` for crates.io:

```toml
xpict-core = { path = "../xpict-core", version = "0.1.4" }
```

```bash
git tag rust-core/v0.1.5 && git push origin rust-core/v0.1.5
# after core is on crates.io:
git tag rust/v0.1.5 && git push origin rust/v0.1.5
```

### One-time setup

1. Create a [crates.io](https://crates.io) account (GitHub login).
2. Confirm crate names are free (`xpict`, `xpict-core`). If taken, rename before first publish.
3. Add repo secret `CARGO_REGISTRY_TOKEN`, or Trusted Publishing for `crates.yml`.
4. Local dry-run (needs system RDKit for `xpict`, not for `xpict-core`):

   ```bash
   cargo publish -p xpict-core --dry-run
   CPLUS_INCLUDE_PATH="$(pwd)/crates/xpict/compat/rdkit" cargo publish -p xpict --dry-run
   ```

`xpict` **requires system RDKit + Boost** at compile time (see `crates/xpict/README.md`).

Do **not** make `xpict-py` or `xpict-wasm` depend on the crates.io `xpict`
package — only on `xpict-core`.

---

## 3. Python — PyPI (`xpict`)

Source under **`python/xpict/`**. Workflow filename **`pypi.yml`** (PyPI Trusted
Publishing).

```bash
git tag py/v0.1.5
git push origin py/v0.1.5
```

### One-time setup

1. Create a PyPI project `xpict` under the lab account.
2. Trusted Publishing → this repo, workflow **`pypi.yml`**.
3. Or store `PYPI_API_TOKEN` as a repo secret.

```bash
uv sync --extra rdkit
uv run maturin build --release -m crates/xpict-py/Cargo.toml
```

---

## 4. Cutting a minor/major (all languages)

1. Bump **major.minor** (and usually reset patch to `0`) in
   `js/package.json`, `pyproject.toml`, `crates/xpict-core`, `crates/xpict`
   (and `xpict-py` / `xpict-wasm` for workspace consistency).
2. Run `bash scripts/check_version_policy.sh`.
3. Tag and push each surface you ship (`js/v0.2.0`, `py/v0.2.0`,
   `rust-core/v0.2.0`, `rust/v0.2.0`) — order: core before native `xpict`.
4. Confirm CI green on the commit you tag.
5. Update consumer pins (e.g. xenosite.org) after registries show the new version.

### Patch (one language)

1. Bump only that package’s patch (keep the same `X.Y`).
2. Tag only that surface (`js/v…` / `py/v…` / `rust-core/v…` / `rust/v…`).

---

## 5. Public API vs future declarative tree

**Ship / document now:** `mol` / `render` / `toSvg` (+ batch stub
`depict({ molecules: [...] }) → Rendered[]`). Live Pydantic is
`xpict.contracts.depict` (`MolSpec` / `DepictSpec`); JSON Schema is
`schema/xpict.schema.json`.

**Not the publish surface yet:** nested `PictSpec` under `xpict.future` /
`schema/future/` — keep refining there; promote fields into `contracts` as
paint support lands.
