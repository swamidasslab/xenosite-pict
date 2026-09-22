# Publishing xpict packages

Three public surfaces share version **0.1.x** today:

| Package | Registry | Status |
| --- | --- | --- |
| `@swamidasslab/xpict` | GitHub Packages (npm) | **Live** — tag `js/v*` or Actions → Publish JS |
| `xpict` (Python) | PyPI | Not automated yet |
| `xpict` + `xpict-core` (Rust) | crates.io | Not automated yet |

The **public API** to document and version is the single-molecule client:

`mol` → `render(opts?)` → `Rendered` → `toSvg(scene)`  

(not the full nested `PictSpec` / ELK diagram engine).

---

## 1. JavaScript — already wired

Workflow: [`.github/workflows/publish-js.yml`](../.github/workflows/publish-js.yml).

```bash
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

## 2. Rust — crates.io (native `xpict` + `xpict-core`)

Publish **`xpict-core` first**, then **`xpict`** (the RDKit-backed public crate). Workspace path deps already carry a `version`, which is what crates.io needs:

```toml
xpict-core = { path = "../xpict-core", version = "0.1.4" }
```

### One-time setup

1. Create a [crates.io](https://crates.io) account (GitHub login).
2. Confirm crate names are free (`xpict`, `xpict-core` were unused at scaffold time). If taken, rename before first publish (e.g. `xenosite-xpict`).
3. Generate an API token: crates.io → Account → API Tokens.
4. Add repo secret `CARGO_REGISTRY_TOKEN` (or use Trusted Publishing — see below).
5. Local dry-run (needs system RDKit for `xpict`, not for `xpict-core`):

   ```bash
   cargo publish -p xpict-core --dry-run
   CPLUS_INCLUDE_PATH="$(pwd)/crates/xpict/compat/rdkit" cargo publish -p xpict --dry-run
   ```

### Trusted Publishing (recommended)

crates.io supports GitHub Actions OIDC ([docs](https://crates.io/docs/trusted-publishing)):

1. On crates.io, for each crate, add a Trusted Publisher pointing at this repo + a publish workflow.
2. Workflow publishes with `crates-io` login via `rust-lang/crates-io-auth-action` (no long-lived token in GitHub secrets).

### Tag / workflow convention (suggested)

| Tag | Publishes |
| --- | --- |
| `rust-core/v0.1.5` | `xpict-core` |
| `rust/v0.1.5` | `xpict` (after core is on crates.io at that version) |

`xpict` **requires system RDKit + Boost** at compile time; document that on the crates.io page (see `crates/xpict/README.md`). `docs.rs` builds will need `[package.metadata.docs.rs]` extras or a stub feature later — fine to ship without docs.rs at first.

### What you do not need for WASM / Python wheels

Do **not** make `xpict-py` or `xpict-wasm` depend on the crates.io `xpict` package. Those keep depending only on `xpict-core` so RDKit never enters the extension / browser blob.

---

## 3. Python — PyPI

Built with **maturin** (`pyproject.toml` → `crates/xpict-py`).

### One-time setup

1. Create a PyPI project `xpict` (or reserved name) under the lab account.
2. Prefer **Trusted Publishing**: PyPI → project → Publishing → add this GitHub repo / workflow environment.
3. Or store `PYPI_API_TOKEN` as a repo secret.

### Suggested workflow (not checked in yet)

```yaml
# on tag py/v*
- uses: PyO3/maturin-action@v1
  with:
    command: publish
    args: --skip-existing
  env:
    MATURIN_PYPI_TOKEN: ${{ secrets.PYPI_API_TOKEN }}
```

Publish **abi3 or per-version wheels** for 3.11–3.13 (matches `requires-python`). Optional extras `rdkit` / `indigo` stay PyPI metadata only — layout backends at the language edge.

Local check:

```bash
uv sync --extra rdkit
uv run maturin build --release -m crates/xpict-py/Cargo.toml
```

---

## 4. Version bump checklist

When cutting a release that should stay aligned across languages:

1. Bump `0.1.x` in `js/package.json`, `pyproject.toml`, `crates/xpict-core`, `crates/xpict`, `crates/xpict-py`, `crates/xpict-wasm`.
2. Tag only the surfaces you intend to publish (`js/v…`, `rust-core/v…`, `rust/v…`, `py/v…`).
3. Confirm CI is green on the commit you tag.
4. Update xenosite.org / consumers’ pins after the registry shows the new version.

---

## 5. Public API vs future declarative tree

**Ship / document now:** `mol` / `render` / `toSvg` (+ optional batch stub `depict({ molecules: [...] }) → Rendered[]`).

**Not the publish surface yet:** nested `PictSpec`, ELK diagrams, reaction/network chrome. Those stay in-tree as the long-term document model; the batch stub is the expandable seam (same mol fields, list-shaped output).
