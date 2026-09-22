//! Chem-label markup → [`ChemGlyph`]s.
//!
//! **Internal dialect** (not KaTeX / Markdown / MathML). Port of the Python
//! `richtext` symbol table + scripts + emphasis, aimed at Liberation Sans
//! glyph outlines. See [`docs/label-markup.md`](../../../docs/label-markup.md).
//!
//! Rules that matter for labels:
//! - Bare ``_`` is literal outside ``$…$`` (so ``my_name`` stays ``my_name``).
//! - ``_{…}`` is always subscript; ``_x`` is subscript only inside ``$…$``.
//! - ``^x`` / ``^{…}`` are always superscript.
//! - ``\alpha`` etc. expand to Unicode; ``**bold**`` / ``*italic*`` set face.

use crate::font::{ChemGlyph, FaceStyle, ScriptRole};

/// Expand chem-label markup into outlined glyphs.
///
/// `base` is the molecule / call default face (`bold_labels` → Bold); markup
/// bold/italic OR on top of it.
pub fn parse_label_markup(raw: &str, base: FaceStyle) -> Vec<ChemGlyph> {
    let (bold0, italic0) = face_flags(base);
    let mut out = Vec::new();
    parse_into(raw, &mut out, bold0, italic0, /*in_math*/ false);
    out
}

fn face_flags(style: FaceStyle) -> (bool, bool) {
    match style {
        FaceStyle::Regular => (false, false),
        FaceStyle::Bold => (true, false),
        FaceStyle::Italic => (false, true),
        FaceStyle::BoldItalic => (true, true),
    }
}

fn push_char(out: &mut Vec<ChemGlyph>, ch: char, role: ScriptRole, bold: bool, italic: bool) {
    let (ch, role) = match unicode_script_base(ch) {
        Some((base, mapped)) if role == ScriptRole::Normal => (base, mapped),
        Some((base, _)) => (base, role),
        None => (ch, role),
    };
    out.push(ChemGlyph {
        ch,
        role,
        face: FaceStyle::from_flags(bold, italic),
    });
}

fn parse_into(src: &str, out: &mut Vec<ChemGlyph>, bold: bool, italic: bool, in_math: bool) {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];

        // $…$ chem zone — enables bare _ scripts; strips delimiters.
        if c == '$' {
            if let Some(j) = find_char(&chars, i + 1, '$') {
                let inner: String = chars[i + 1..j].iter().collect();
                parse_into(&inner, out, bold, italic, true);
                i = j + 1;
                continue;
            }
            push_char(out, c, ScriptRole::Normal, bold, italic);
            i += 1;
            continue;
        }

        // Backslash: escape or symbol / style command.
        if c == '\\' {
            if i + 1 >= chars.len() {
                push_char(out, '\\', ScriptRole::Normal, bold, italic);
                i += 1;
                continue;
            }
            let nxt = chars[i + 1];
            if matches!(nxt, '\\' | '{' | '}' | '$' | '*' | '_' | '^') {
                push_char(out, nxt, ScriptRole::Normal, bold, italic);
                i += 2;
                continue;
            }
            if nxt.is_ascii_alphabetic() {
                let (name, after) = read_command_name(&chars, i + 1);
                if let Some(sym) = lookup_symbol(&name) {
                    for ch in sym.chars() {
                        push_char(out, ch, ScriptRole::Normal, bold, italic);
                    }
                    i = after;
                    continue;
                }
                if let Some(style_kind) = lookup_style_cmd(&name) {
                    if after < chars.len() && chars[after] == '{' {
                        if let Some((body, end)) = read_braced(&chars, after) {
                            let (nb, ni) = match style_kind {
                                StyleKind::Bold => (true, italic),
                                StyleKind::Italic => (bold, true),
                            };
                            parse_into(&body, out, nb, ni, in_math);
                            i = end;
                            continue;
                        }
                    }
                    // Unknown body — emit literal \name.
                    push_char(out, '\\', ScriptRole::Normal, bold, italic);
                    for ch in name.chars() {
                        push_char(out, ch, ScriptRole::Normal, bold, italic);
                    }
                    i = after;
                    continue;
                }
                // Unknown command — keep literal backslash + name.
                push_char(out, '\\', ScriptRole::Normal, bold, italic);
                for ch in name.chars() {
                    push_char(out, ch, ScriptRole::Normal, bold, italic);
                }
                i = after;
                continue;
            }
            push_char(out, '\\', ScriptRole::Normal, bold, italic);
            i += 1;
            continue;
        }

        // Markdown **bold**
        if c == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
            if let Some(j) = find_str(&chars, i + 2, &['*', '*']) {
                let inner: String = chars[i + 2..j].iter().collect();
                parse_into(&inner, out, true, italic, in_math);
                i = j + 2;
                continue;
            }
            push_char(out, '*', ScriptRole::Normal, bold, italic);
            push_char(out, '*', ScriptRole::Normal, bold, italic);
            i += 2;
            continue;
        }

        // Markdown *italic* (single *; not part of **)
        if c == '*' {
            if let Some(j) = find_italic_close(&chars, i + 1) {
                let inner: String = chars[i + 1..j].iter().collect();
                parse_into(&inner, out, bold, true, in_math);
                i = j + 1;
                continue;
            }
            push_char(out, '*', ScriptRole::Normal, bold, italic);
            i += 1;
            continue;
        }

        // Scripts: ^ always; _ bare only in math; _{…} always.
        if c == '^' || c == '_' {
            let allow_bare = c == '^' || in_math;
            let role = if c == '_' {
                ScriptRole::Subscript
            } else {
                ScriptRole::Superscript
            };
            if i + 1 < chars.len() && chars[i + 1] == '{' {
                let (body, end) = read_braced(&chars, i + 1).unwrap_or_else(|| (String::new(), i + 1));
                for ch in body.chars() {
                    push_char(out, ch, role, bold, italic);
                }
                i = end;
                continue;
            }
            if allow_bare && i + 1 < chars.len() {
                push_char(out, chars[i + 1], role, bold, italic);
                i += 2;
                continue;
            }
            // Literal underscore / caret.
            push_char(out, c, ScriptRole::Normal, bold, italic);
            i += 1;
            continue;
        }

        push_char(out, c, ScriptRole::Normal, bold, italic);
        i += 1;
    }
}

#[derive(Clone, Copy)]
enum StyleKind {
    Bold,
    Italic,
}

fn lookup_style_cmd(name: &str) -> Option<StyleKind> {
    match name {
        "textbf" | "mathbf" | "bf" => Some(StyleKind::Bold),
        "textit" | "mathit" | "it" | "emph" => Some(StyleKind::Italic),
        _ => None,
    }
}

fn lookup_symbol(name: &str) -> Option<&'static str> {
    // Longest-match is handled by read_command_name callers via longest known prefix.
    SYMBOLS.iter().find(|(n, _)| *n == name).map(|(_, u)| *u)
}

fn read_command_name(chars: &[char], start: usize) -> (String, usize) {
    let mut j = start;
    while j < chars.len() && chars[j].is_ascii_alphabetic() {
        j += 1;
    }
    let raw: String = chars[start..j].iter().collect();
    // Longest match against known symbols / style cmds.
    let mut name = raw.clone();
    let mut end = j;
    while !name.is_empty()
        && lookup_symbol(&name).is_none()
        && lookup_style_cmd(&name).is_none()
    {
        name.pop();
        end -= 1;
    }
    if name.is_empty() {
        (raw, j)
    } else {
        (name, end)
    }
}

fn read_braced(chars: &[char], open_idx: usize) -> Option<(String, usize)> {
    if open_idx >= chars.len() || chars[open_idx] != '{' {
        return None;
    }
    let mut depth = 0i32;
    let mut j = open_idx;
    while j < chars.len() {
        match chars[j] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let body: String = chars[open_idx + 1..j].iter().collect();
                    return Some((body, j + 1));
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

fn find_char(chars: &[char], from: usize, target: char) -> Option<usize> {
    (from..chars.len()).find(|&i| chars[i] == target)
}

fn find_str(chars: &[char], from: usize, pat: &[char]) -> Option<usize> {
    let n = pat.len();
    if n == 0 || from + n > chars.len() {
        return None;
    }
    (from..=chars.len() - n).find(|&i| chars[i..i + n] == *pat)
}

fn find_italic_close(chars: &[char], from: usize) -> Option<usize> {
    let mut j = from;
    while j < chars.len() {
        if chars[j] == '\\' {
            j += 2;
            continue;
        }
        if chars[j] == '*' {
            // Don't close on **
            if j + 1 < chars.len() && chars[j + 1] == '*' {
                j += 2;
                continue;
            }
            return Some(j);
        }
        j += 1;
    }
    None
}

fn unicode_script_base(ch: char) -> Option<(char, ScriptRole)> {
    let sub = match ch {
        '₀' => '0',
        '₁' => '1',
        '₂' => '2',
        '₃' => '3',
        '₄' => '4',
        '₅' => '5',
        '₆' => '6',
        '₇' => '7',
        '₈' => '8',
        '₉' => '9',
        _ => '\0',
    };
    if sub != '\0' {
        return Some((sub, ScriptRole::Subscript));
    }
    let sup = match ch {
        '⁰' => '0',
        '¹' => '1',
        '²' => '2',
        '³' => '3',
        '⁴' => '4',
        '⁵' => '5',
        '⁶' => '6',
        '⁷' => '7',
        '⁸' => '8',
        '⁹' => '9',
        '⁺' => '+',
        '⁻' => '-',
        _ => '\0',
    };
    if sup != '\0' {
        return Some((sup, ScriptRole::Superscript));
    }
    None
}

// TeX-ish name → Unicode (Liberation Sans coverage). Same set as Python richtext.
const SYMBOLS: &[(&str, &str)] = &[
    ("alpha", "α"),
    ("beta", "β"),
    ("gamma", "γ"),
    ("delta", "δ"),
    ("epsilon", "ε"),
    ("varepsilon", "ε"),
    ("zeta", "ζ"),
    ("eta", "η"),
    ("theta", "θ"),
    ("vartheta", "ϑ"),
    ("iota", "ι"),
    ("kappa", "κ"),
    ("lambda", "λ"),
    ("mu", "μ"),
    ("nu", "ν"),
    ("xi", "ξ"),
    ("omicron", "ο"),
    ("pi", "π"),
    ("varpi", "ϖ"),
    ("rho", "ρ"),
    ("varrho", "ϱ"),
    ("sigma", "σ"),
    ("varsigma", "ς"),
    ("tau", "τ"),
    ("upsilon", "υ"),
    ("phi", "φ"),
    ("varphi", "ϕ"),
    ("chi", "χ"),
    ("psi", "ψ"),
    ("omega", "ω"),
    ("Alpha", "Α"),
    ("Beta", "Β"),
    ("Gamma", "Γ"),
    ("Delta", "Δ"),
    ("Epsilon", "Ε"),
    ("Zeta", "Ζ"),
    ("Eta", "Η"),
    ("Theta", "Θ"),
    ("Iota", "Ι"),
    ("Kappa", "Κ"),
    ("Lambda", "Λ"),
    ("Mu", "Μ"),
    ("Nu", "Ν"),
    ("Xi", "Ξ"),
    ("Omicron", "Ο"),
    ("Pi", "Π"),
    ("Rho", "Ρ"),
    ("Sigma", "Σ"),
    ("Tau", "Τ"),
    ("Upsilon", "Υ"),
    ("Phi", "Φ"),
    ("Chi", "Χ"),
    ("Psi", "Ψ"),
    ("Omega", "Ω"),
    ("degree", "°"),
    ("circ", "°"),
    ("pm", "±"),
    ("mp", "∓"),
    ("times", "×"),
    ("cdot", "·"),
    ("ast", "∗"),
    ("dagger", "†"),
    ("ddagger", "‡"),
    ("prime", "′"),
    ("infty", "∞"),
    ("approx", "≈"),
    ("neq", "≠"),
    ("ne", "≠"),
    ("leq", "≤"),
    ("le", "≤"),
    ("geq", "≥"),
    ("ge", "≥"),
    ("rightarrow", "→"),
    ("to", "→"),
    ("leftarrow", "←"),
    ("leftrightarrow", "↔"),
    ("uparrow", "↑"),
    ("downarrow", "↓"),
    ("micro", "µ"),
    ("AA", "Å"),
    ("angstrom", "Å"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn text_roles(raw: &str) -> Vec<(char, ScriptRole, FaceStyle)> {
        parse_label_markup(raw, FaceStyle::Regular)
            .into_iter()
            .map(|g| (g.ch, g.role, g.face))
            .collect()
    }

    #[test]
    fn my_name_literal_underscore() {
        let g = parse_label_markup("my_name", FaceStyle::Regular);
        assert_eq!(g.len(), 7);
        assert!(g.iter().all(|x| x.role == ScriptRole::Normal));
        assert_eq!(g.iter().map(|x| x.ch).collect::<String>(), "my_name");
    }

    #[test]
    fn bare_r_1_literal_dollar_r_1_subscript() {
        let bare = parse_label_markup("R_1", FaceStyle::Regular);
        assert_eq!(bare.iter().map(|x| x.ch).collect::<String>(), "R_1");
        assert_eq!(bare[1].role, ScriptRole::Normal);

        let math = parse_label_markup("$R_1$", FaceStyle::Regular);
        assert_eq!(math.len(), 2);
        assert_eq!(math[0].ch, 'R');
        assert_eq!(math[1].ch, '1');
        assert_eq!(math[1].role, ScriptRole::Subscript);
    }

    #[test]
    fn braced_sub_outside_math() {
        let g = parse_label_markup("H_{2}", FaceStyle::Regular);
        assert_eq!(g.len(), 2);
        assert_eq!(g[1].role, ScriptRole::Subscript);
        assert_eq!(g[1].ch, '2');
    }

    #[test]
    fn caret_super_bare() {
        let g = parse_label_markup("R^2", FaceStyle::Regular);
        assert_eq!(g[1].role, ScriptRole::Superscript);
        assert_eq!(g[1].ch, '2');
    }

    #[test]
    fn greek_and_delta() {
        let g = parse_label_markup(r"\alpha-\beta", FaceStyle::Regular);
        assert_eq!(g.iter().map(|x| x.ch).collect::<String>(), "α-β");
        let d = parse_label_markup(r"\Delta\DeltaG", FaceStyle::Regular);
        assert_eq!(d.iter().map(|x| x.ch).collect::<String>(), "ΔΔG");
    }

    #[test]
    fn markdown_bold_italic() {
        let g = parse_label_markup("**Et**OH", FaceStyle::Regular);
        assert_eq!(g[0].face, FaceStyle::Bold);
        assert_eq!(g[1].face, FaceStyle::Bold);
        assert_eq!(g[2].face, FaceStyle::Regular);
        let it = parse_label_markup("*cis*", FaceStyle::Regular);
        assert!(it.iter().all(|x| x.face == FaceStyle::Italic));
    }

    #[test]
    fn math_with_alpha_sub() {
        let g = parse_label_markup(r"$\alpha_D$", FaceStyle::Regular);
        assert_eq!(g[0].ch, 'α');
        assert_eq!(g[1].ch, 'D');
        assert_eq!(g[1].role, ScriptRole::Subscript);
    }

    #[test]
    fn base_face_or() {
        let g = parse_label_markup("*cis*", FaceStyle::Bold);
        assert!(g.iter().all(|x| x.face == FaceStyle::BoldItalic));
    }

    #[test]
    fn roles_smoke() {
        let t = text_roles("$R^{2+}$");
        assert_eq!(t[0].1, ScriptRole::Normal);
        assert_eq!(t[1].1, ScriptRole::Superscript);
        assert_eq!(t[2].1, ScriptRole::Superscript);
    }
}
