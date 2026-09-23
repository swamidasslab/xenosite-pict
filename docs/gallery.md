# Gallery

Publication-style SVG from the **live nested document** (`type: "mol"` /
`type: "group"`).

Regenerate:

```bash
uv run python scripts/generate_doc_examples.py
```

Label scripts: [Label markup](label-markup.md) (CX braced aliases; simple
`star_labels` on the single-mol client).

## Structures

<div class="gallery-grid" markdown="0">
  <figure>
    <img src="assets/examples/aspirin.svg" alt="Aspirin" />
    <figcaption>Aspirin</figcaption>
  </figure>
  <figure>
    <img src="assets/examples/ibuprofen.svg" alt="Ibuprofen" />
    <figcaption>Ibuprofen</figcaption>
  </figure>
  <figure>
    <img src="assets/examples/caffeine.svg" alt="Caffeine" />
    <figcaption>Caffeine</figcaption>
  </figure>
  <figure>
    <img src="assets/examples/penicillin_g.svg" alt="Penicillin G" />
    <figcaption>Penicillin G</figcaption>
  </figure>
  <figure>
    <img src="assets/examples/glucose.svg" alt="Glucose" />
    <figcaption>Glucose (stereo SMILES)</figcaption>
  </figure>
  <figure>
    <img src="assets/examples/phenol.svg" alt="Phenol" />
    <figcaption>Phenol</figcaption>
  </figure>
  <figure>
    <img src="assets/examples/benzene.svg" alt="Benzene" />
    <figcaption>Benzene</figcaption>
  </figure>
  <figure>
    <img src="assets/examples/ethanol.svg" alt="Ethanol" />
    <figcaption>Ethanol</figcaption>
  </figure>
</div>

## Markush labels

CX braced markup on the live document (`R_{1}` inside the trailer):

<div class="gallery-grid" markdown="0">
  <figure>
    <img src="assets/examples/markush.svg" alt="Markush via CX R_{1}" />
    <figcaption>CX — <code>|$R_{1};;;;;$|</code> → R₁</figcaption>
  </figure>
</div>

## Sparse shade

Most atom scores are zero; a few sites are hot (SoM-style):

<div class="gallery-grid" markdown="0">
  <figure>
    <img src="assets/examples/aspirin_shade.svg" alt="Aspirin sparse shade" />
    <figcaption>Aspirin — sparse atom shade</figcaption>
  </figure>
  <figure>
    <img src="assets/examples/phenol_shade.svg" alt="Phenol oxygen shade" />
    <figcaption>Phenol — oxygen hot</figcaption>
  </figure>
  <figure>
    <img src="assets/examples/pyridine_shade.svg" alt="Pyridine nitrogen shade" />
    <figcaption>Pyridine — nitrogen hot</figcaption>
  </figure>
</div>
