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

Tiny API — RDKit is loaded and initialized for you (npm module in **Node /
bundlers**, auto `<script>` inject in the **browser**). Present a template
mol, then `draw`; alignment uses RDKit under the hood. Atom coords are in
the same SCALE / viewBox space as the SVG ink.

```ts
import { init, presentTemplate, draw } from "@swamidasslab/xpict";

await init(); // Node: pass { wasm: bytes } if fetch isn't available
presentTemplate("c1ccccc1");

const { svg, atoms, width, height, imgDataUri } = await draw("Cc1ccccc1", {
  mark_atoms: [0],
});
// atoms[i].{x,y} match SVG positions (pad-translated viewBox / SCALE units)
```

Works server-side (Node) the same way — no DOM required.

Low-level paint (coords already in hand):

```ts
import { initNative, depictMolecule, sceneToSvg } from "@swamidasslab/xpict";

await initNative();
const scene = JSON.parse(depictMolecule(JSON.stringify(moleculeIn)));
const svg = sceneToSvg(scene);
```

Brand name `@xenosite/xpict` can be an npmjs publish later; GitHub Packages
requires the `@swamidasslab` scope.
