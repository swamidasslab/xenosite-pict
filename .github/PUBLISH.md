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
| JS patch only | `js/v0.3.1` (`Z≥1`) → GitHub Packages `@swamidasslab/xpict` | [`publish-js.yml`](https://github.com/swamidasslab/xenosite-pict/blob/main/.github/workflows/publish-js.yml) |
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

   ```bash
   make bump VERSION=0.2.0
   ```

   That updates JS, Python (`pyproject` + `__version__`), and all Rust crates
   (including the `xpict` → `xpict-core` dep), then runs the product version
   policy check. `release.yml` runs the same recipe on tag so a `release/v*`
   push publishes every surface at that version.
2. Merge; CI green.
3. Tag and push:

   ```bash
   git tag release/v0.2.0
   git push origin release/v0.2.0
   ```

4. [`release.yml`](https://github.com/swamidasslab/xenosite-pict/blob/main/.github/workflows/release.yml) publishes **xpict-core →
   xpict → JS**, then **dispatches [`pypi.yml`](https://github.com/swamidasslab/xenosite-pict/blob/main/.github/workflows/pypi.yml)**
   so Python still ships on major/minor via Trusted Publishing (`pypi.yml` +
   environment `pypi`). Environments `crates` / `npm` must allow `release/v*`
   tags; environment `pypi` must allow `workflow_dispatch` / `py/v*`.

Do **not** tag `js/v0.2.0` alone — it will fail the patch-only gate.

---

## 1. JavaScript (patch) — GitHub Packages (temporary)

**For now** JS ships to **GitHub Packages**, not npmjs.org (OIDC / npmjs TBD).

Published name: **`@swamidasslab/xpict`** (GitHub requires the scope to match
the repo owner). Source `js/package.json` keeps `"name": "@xenosite/xpict"`;
the publish workflow rewrites the name at publish time.

```bash
# Z must be >= 1. In-tree major.minor must already match others.
git tag js/v0.3.1
git push origin js/v0.3.1
```

Auth is `GITHUB_TOKEN` (`packages: write`). No `NPM_TOKEN`.

Consumers (private package — need `read:packages`):

```bash
# .npmrc
@swamidasslab:registry=https://npm.pkg.github.com
//npm.pkg.github.com/:_authToken=${GITHUB_TOKEN}

npm install @swamidasslab/xpict
```

Package URL:
https://github.com/swamidasslab/xenosite-pict/pkgs/npm/xpict

### Later: npmjs.org + OIDC

1. One-time stub `@xenosite/xpict@0.0.0` on npmjs (token), then Trusted
   Publisher → this repo, workflow `publish-js.yml` / `release.yml`,
   environment optional.
2. Point workflows back at `registry.npmjs.org`, drop the name rewrite,
   set `permissions.id-token: write`, publish without `NODE_AUTH_TOKEN`.

---

## 2. Rust patches — crates.io

Publish **`xpict-core` first**, then **`xpict`** when both need a patch.

```toml
xpict-core = { path = "../xpict-core", version = "0.2.0" }
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
   make publish-dry-core
   make publish-dry-rust
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

### Auth: Trusted Publishing only (no API token)

Publisher on PyPI: workflow **`pypi.yml`**, environment **`pypi`**.

Workflows omit `MATURIN_PYPI_TOKEN` so maturin uses OIDC. Passing an empty
token (e.g. unset `PYPI_API_TOKEN` secret) forces password auth and **breaks**
Trusted Publishing. **`release.yml` does not upload to PyPI.**

### One-time setup

1. Create a PyPI project `xpict` (or pending publisher before first upload).
2. Trusted Publishing → this repo, workflow **`pypi.yml`**, environment **`pypi`**.
3. GitHub Environment **`pypi`**: allow tags `py/v*` and workflow_dispatch (or
   “No restriction”). No `PYPI_API_TOKEN` secret.

After a product `release/vX.Y.0`, **`release.yml` dispatches this workflow
automatically**. Patches: `git tag py/vX.Y.Z && git push`. Manual:
Actions → pypi → Run workflow.

```bash
make sync
make publish-dry-python
```

---

## 4. Public API vs future declarative tree

**Ship / document now:** nested ``DepictSpec`` (``type: "mol"`` /
``type: "group"`` + ``children``) — strict subset of future PictSpec. Also
``mol`` / ``render`` / ``toSvg`` for single-mol callers. Live JSON Schema from
Rust (``make types`` → ``schema/xpict.schema.json``); Python hand models in
``xpict.contracts.depict``; JS types in ``js/src/generated``.

**Not the publish surface yet:** full nested ``PictSpec`` under ``xpict.future`` /
``schema/future/``.
