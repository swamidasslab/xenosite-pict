# Chem label markup

xpict uses a **small internal dialect** for atom / star labels — not KaTeX,
MathJax, or a Markdown engine. It lives in Rust (`xpict-core::markup`) so
JavaScript, Python, and native Rust share one path into Liberation Sans glyph
outlines.

Structural labels (`NH2`, charges) emit the same markup (`H_{2}`, `^{+}`) and
go through the same parser. There is no second script pathway.

## Why not an existing crate?

Math crates (`pulldown-latex`, `tex2math`, …) emit **MathML**. xpict paints
**glyph paths** with fake sub/superscripts so labels match bond ink. Pulling a
full math engine would add weight and a second sink without removing our
outline step. We ported the Python `richtext` symbol table + a tiny script /
emphasis grammar instead.

## Dialect

| Input | Result | Notes |
| --- | --- | --- |
| `my_name` | `my_name` | Bare `_` is literal outside `$…$` |
| `R_1` | `R_1` | Same — not a subscript |
| `$R_1$` | R₁ | Bare `_` scripts **only** inside `$…$` |
| `H_{2}` / `$R_{10}$` | H₂ / R₁₀ | Braced `_{…}` always subscripts |
| `R^2` / `R^{2+}` | R² / R²⁺ | `^` always superscripts |
| `\alpha` `\beta` `\Delta` … | α β Δ | Same names as Python `richtext` |
| `**bold**` / `*italic*` | face flags | Markdown emphasis |
| `\textbf{…}` `\textit{…}` | face flags | LaTeX-ish style cmds |
| `\_` `\*` `\^` `\$` `\\` | literals | Escapes |

Examples:

```text
$R_1$
R^2
$\alpha$-D-Glc
**R**^2
$\beta_{D}$
```

Molecule-level `bold_labels` sets the base face; markup bold/italic OR on top
(so `*cis*` with bold labels → BoldItalic).

## Not supported (on purpose)

Fractions, matrices, real TeX layout, Markdown `_emphasis_` (would break
`my_name`). Put chemistry scripts in `$…$` or use `_{…}` / `^{…}`.
