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

<div class="gallery-grid" markdown>

<figure markdown="span">
![Aspirin](assets/examples/aspirin.svg)
<figcaption>Aspirin</figcaption>
</figure>

<figure markdown="span">
![Ibuprofen](assets/examples/ibuprofen.svg)
<figcaption>Ibuprofen</figcaption>
</figure>

<figure markdown="span">
![Caffeine](assets/examples/caffeine.svg)
<figcaption>Caffeine</figcaption>
</figure>

<figure markdown="span">
![Penicillin G](assets/examples/penicillin_g.svg)
<figcaption>Penicillin G</figcaption>
</figure>

<figure markdown="span">
![Glucose](assets/examples/glucose.svg)
<figcaption>Glucose (stereo SMILES)</figcaption>
</figure>

<figure markdown="span">
![Phenol](assets/examples/phenol.svg)
<figcaption>Phenol</figcaption>
</figure>

<figure markdown="span">
![Benzene](assets/examples/benzene.svg)
<figcaption>Benzene</figcaption>
</figure>

<figure markdown="span">
![Ethanol](assets/examples/ethanol.svg)
<figcaption>Ethanol</figcaption>
</figure>

</div>

## Markush labels

CX braced markup on the live document (`R_{1}` inside the trailer):

<div class="gallery-grid" markdown>

<figure markdown="span">
![Markush via CX R₁](assets/examples/markush.svg)
<figcaption>CX — <code>|$R_{1};;;;;$|</code> → R₁</figcaption>
</figure>

</div>

## Sparse shade

Most atom scores are zero; a few sites are hot (SoM-style):

<div class="gallery-grid" markdown>

<figure markdown="span">
![Aspirin sparse shade](assets/examples/aspirin_shade.svg)
<figcaption>Aspirin — sparse atom shade</figcaption>
</figure>

<figure markdown="span">
![Phenol oxygen shade](assets/examples/phenol_shade.svg)
<figcaption>Phenol — oxygen hot</figcaption>
</figure>

<figure markdown="span">
![Pyridine nitrogen shade](assets/examples/pyridine_shade.svg)
<figcaption>Pyridine — nitrogen hot</figcaption>
</figure>

</div>
