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
| Product cut (all surfaces) | `release/v0.2.0` | [`release.yml`](https://github.com/swamidasslab/xenosite-pict/blob/main/.github/workflows/release.yml) |
| JS patch only | `js/v0.1.5` (`Z≥1`) | [`publish-js.yml`](https://github.com/swamidasslab/xenosite-pict/blob/main/.github/workflows/publish-js.yml) |
| Python patch only | `py/v0.1.5` | [`pypi.yml`](https://github.com/swamidasslab/xenosite-pict/blob/main/.github/workflows/pypi.yml) |
| `xpict-core` patch | `rust-core/v0.1.5` | [`crates.yml`](https://github.com/swamidasslab/xenosite-pict/blob/main/.github/workflows/crates.yml) |
| Rust `xpict` patch | `rust/v0.1.5` | [`crates.yml`](https://github.com/swamidasslab/xenosite-pict/blob/main/.github/workflows/crates.yml) |

CI runs [`scripts/check_version_policy.sh`](https://github.com/swamidasslab/xenosite-pict/blob/main/scripts/check_version_policy.sh).

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

5. [`release.yml`](https://github.com/swamidasslab/xenosite-pict/blob/main/.github/workflows/release.yml) publishes **xpict-core →
   xpict → JS → Python** at that version (or Actions → **release** workflow
   dispatch).

Do **not** tag `js/v0.2.0` alone — it will fail the patch-only gate.

---

## 1. JavaScript (patch) — npmjs.org

Package: **`@xenosite/xpict`** on the public npm registry.

```bash
# Z must be >= 1. In-tree major.minor must already match others.
git tag js/v0.1.5
git push origin js/v0.1.5
```

### One-time setup

1. Create an [npmjs.org](https://www.npmjs.com) user (or org **`xenosite`**).
2. Claim / create the **`@xenosite`** scope and grant this account publish rights
   (org: add the publishing user as a member with publish permission).
   Prefer this scope so it stays aligned if the GitHub repo moves under a
   `xenosite` org later.
3. npm → Access Tokens → **Automation** token (or Granular with publish for
   `@xenosite/xpict`).
4. GitHub repo **Settings → Environments → `npm`** → Environment secrets →
   **`NPM_TOKEN`** = that token.
5. On the same environment, set **Deployment branches and tags** so tag
   publishes can run (e.g. allow tags `js/v*` and `release/v*`, or “No
   restriction”). “Protected branches only” blocks tag-triggered workflows.
6. First publish creates https://www.npmjs.com/package/@xenosite/xpict

Consumers:

```bash
npm install @xenosite/xpict
```

No GitHub Packages `.npmrc` is required. xenosite.org should depend on the
npmjs package the same way.

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
3. GitHub repo **Settings → Environments → `crates`** → Environment secrets →
   **`CARGO_REGISTRY_TOKEN`** (used by `crates.yml` and `release.yml`).
4. On the same environment, allow deployment from tags `rust-core/v*`,
   `rust/v*`, and `release/v*` (or “No restriction”). “Protected branches
   only” blocks tag-triggered workflows.
5. Local dry-run:

   ```bash
   cargo publish -p xpict-core --dry-run
   CPLUS_INCLUDE_PATH="$(pwd)/crates/xpict/compat/rdkit" cargo publish -p xpict --dry-run
   ```

`xpict` **requires system RDKit + Boost** at compile time (see `crates/xpict/README.md`).

Do **not** make `xpict-py` / `xpict-wasm` depend on crates.io `xpict` — only `xpict-core`.

---

## 3. Python patch — PyPI

Source under **`python/xpict/`**. Workflow filename **`pypi.yml`**.

Ships **abi3** wheels (`cp311-abi3`) for:

| OS | Architectures |
| --- | --- |
| Linux (manylinux) | x86_64, aarch64 |
| macOS | x86_64 (`macos-15-intel`), aarch64 (`macos-latest`) |
| Windows | x64, aarch64 (`windows-11-arm`) |

Plus an sdist. One wheel per platform covers CPython 3.11–3.13.

```bash
git tag py/v0.1.5
git push origin py/v0.1.5
```

### One-time setup

1. Create a PyPI project `xpict`.
2. Trusted Publishing → this repo, workflow **`pypi.yml`** (and allow
   `release.yml` if using OIDC there too). Point the publisher at the **`pypi`**
   GitHub Environment if you use one.
3. Or add **`PYPI_API_TOKEN`** as an environment secret on **`pypi`**
   (used by `pypi.yml` and `release.yml`).
4. Allow deployment from tags `py/v*` and `release/v*` (or “No restriction”).

```bash
uv sync --extra rdkit
uv run maturin build --release -m crates/xpict-py/Cargo.toml
```

---

## 4. Public API vs future declarative tree

**Ship / document now:** nested ``DepictSpec`` (``type: "mol"`` /
``type: "group"`` + ``children``) — strict subset of future PictSpec. Also
``mol`` / ``render`` / ``toSvg`` for single-mol callers. Pydantic:
``xpict.contracts.depict``; JSON Schema: ``schema/xpict.schema.json``.

**Not the publish surface yet:** full nested ``PictSpec`` under ``xpict.future`` /
``schema/future/``.
