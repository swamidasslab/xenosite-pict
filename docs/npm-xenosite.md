# `@swamidasslab/xpict` for xenosite.org

The website installs the WASM/TS package from **GitHub Packages** (scope must
match the GitHub org: `swamidasslab`).

## Publish (this repo)

```bash
# After merging to main:
git tag js/v0.1.1
git push origin js/v0.1.1
```

Or **Actions → Publish JS package → Run workflow**.

CI builds wasm + `tsc`, runs smoke tests, and `npm publish`es to
`https://npm.pkg.github.com`.

## Install (xenosite app / CI)

`.npmrc` in the website repo:

```
@swamidasslab:registry=https://npm.pkg.github.com
//npm.pkg.github.com/:_authToken=${NPM_TOKEN}
```

`package.json`:

```json
{
  "dependencies": {
    "@swamidasslab/xpict": "^0.1.0"
  }
}
```

### GitHub Actions (xenosite)

```yaml
permissions:
  contents: read
  packages: read

steps:
  - uses: actions/setup-node@v4
    with:
      node-version: "22"
      registry-url: https://npm.pkg.github.com
      scope: "@swamidasslab"
  - run: npm ci
    env:
      NODE_AUTH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

**First time:** after the first publish, open the package on GitHub
(`swamidasslab/xenosite-pict` → Packages → `@swamidasslab/xpict`) →
**Package settings → Manage Actions access** → grant the **xenosite** repo
read access (same-org `GITHUB_TOKEN` can then install it).

Locally, use a PAT with `read:packages` as `NPM_TOKEN` / `NODE_AUTH_TOKEN`.

## App usage

```ts
import { xpict } from "@swamidasslab/xpict";

await xpict.init(); // Node: pass { wasm: bytes } if fetch isn't available

const mol = xpict.mol("c1ccccc1");
const rendered = await xpict.render(mol);
// tweak rendered.scene (paths/circles/…) then:
const svg = xpict.toSvg(rendered.scene);
const img = xpict.toImgDataUri(rendered.scene);

rendered.svg_coords; // match scene ink
rendered.coords;     // SCALE, pre-pad

const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), {
  align_to: mol, // or align_to: rendered
  mark_atoms: [0],
});
```

`mol` / `rendered` both carry `frame_molblock` for alignment. RDKit is hidden.
Works server-side (Node) the same way.

Low-level wasm: `import { … } from "@swamidasslab/xpict/native"`.
