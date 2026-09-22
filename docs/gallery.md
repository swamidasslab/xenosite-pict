# Gallery

Publication-style SVG depictions from `xpict` (RDKit layout + Rust paint).

<div class="gallery-grid" markdown="0">
  <figure>
    <img src="assets/examples/aspirin.svg" alt="Aspirin" />
    <figcaption>Aspirin — <code>CC(=O)Oc1ccccc1C(=O)O</code></figcaption>
  </figure>
  <figure>
    <img src="assets/examples/caffeine.svg" alt="Caffeine" />
    <figcaption>Caffeine</figcaption>
  </figure>
  <figure>
    <img src="assets/examples/glucose.svg" alt="Glucose" />
    <figcaption>Glucose (stereo SMILES)</figcaption>
  </figure>
  <figure>
    <img src="assets/examples/ethanol.svg" alt="Ethanol" />
    <figcaption>Ethanol — heteroatom <code>OH</code></figcaption>
  </figure>
  <figure>
    <img src="assets/examples/benzene.svg" alt="Benzene" />
    <figcaption>Benzene</figcaption>
  </figure>
  <figure>
    <img src="assets/examples/markush.svg" alt="Markush" />
    <figcaption>Markush — CXSMILES <code>$R1$</code></figcaption>
  </figure>
  <figure>
    <img src="assets/examples/phenol_marks.svg" alt="Phenol with mark" />
    <figcaption>Phenol with atom mark</figcaption>
  </figure>
</div>

Regenerate SVGs with:

```bash
uv run python scripts/generate_doc_examples.py
```
