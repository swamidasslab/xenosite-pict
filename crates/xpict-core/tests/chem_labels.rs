//! Usage-facing chem-label tests (public API only).
//!
//! These mirror [`docs/label-markup.md`] and the strings backends actually
//! emit (`NH2`, `OH`, `NH4+`, Markush `$R_1$`). Failures here are bugs anyone
//! hitting `compose_label` / `place_label` / `parse_label_markup` would see.

use xpict_core::font::{FaceStyle, ScriptRole};
use xpict_core::labels::{
    compose_label, place_label, split_label, LabelSide,
};
use xpict_core::metrics::FONT_PX;
use xpict_core::parse_label_markup;

fn chars(raw: &str) -> String {
    parse_label_markup(raw, FaceStyle::Regular)
        .into_iter()
        .map(|g| g.ch)
        .collect()
}

fn roles(raw: &str) -> Vec<(char, ScriptRole)> {
    parse_label_markup(raw, FaceStyle::Regular)
        .into_iter()
        .map(|g| (g.ch, g.role))
        .collect()
}

/// Dialect table from docs/label-markup.md — parser contract.
#[test]
fn doc_dialect_parse_label_markup() {
    // Bare underscore stays literal (filenames / aliases).
    assert_eq!(chars("my_name"), "my_name");
    assert_eq!(chars("R_1"), "R_1");
    assert!(roles("R_1").iter().all(|(_, r)| *r == ScriptRole::Normal));

    // Math zone enables bare `_`.
    assert_eq!(roles("$R_1$"), vec![('R', ScriptRole::Normal), ('1', ScriptRole::Subscript)]);

    // Braced sub always works; multi-digit.
    assert_eq!(
        roles("H_{2}"),
        vec![('H', ScriptRole::Normal), ('2', ScriptRole::Subscript)]
    );
    assert_eq!(
        roles("$R_{10}$"),
        vec![
            ('R', ScriptRole::Normal),
            ('1', ScriptRole::Subscript),
            ('0', ScriptRole::Subscript),
        ]
    );

    // Caret always superscripts.
    assert_eq!(roles("R^2"), vec![('R', ScriptRole::Normal), ('2', ScriptRole::Superscript)]);
    assert_eq!(
        roles("R^{2+}"),
        vec![
            ('R', ScriptRole::Normal),
            ('2', ScriptRole::Superscript),
            ('+', ScriptRole::Superscript),
        ]
    );

    // Doc examples.
    assert_eq!(chars(r"$\alpha$-D-Glc"), "α-D-Glc");
    assert_eq!(chars(r"$\beta_{D}$"), "βD");
    assert_eq!(
        roles(r"$\beta_{D}$"),
        vec![('β', ScriptRole::Normal), ('D', ScriptRole::Subscript)]
    );

    let bold_r = parse_label_markup("**R**^2", FaceStyle::Regular);
    assert_eq!(bold_r[0].ch, 'R');
    assert_eq!(bold_r[0].face, FaceStyle::Bold);
    assert_eq!(bold_r[1].ch, '2');
    assert_eq!(bold_r[1].role, ScriptRole::Superscript);
    assert_eq!(bold_r[1].face, FaceStyle::Regular);
}

/// Same strings through split → compose (what backends feed place_label).
#[test]
fn doc_and_backend_labels_compose() {
    let cases: &[(&str, &str)] = &[
        ("my_name", "my_name"),
        ("R_1", "R_1"),
        ("$R_1$", "R₁"),
        ("H_{2}", "H₂"),
        ("$R_{10}$", "R₁₀"),
        ("R^2", "R²"),
        ("R^{2+}", "R²⁺"),
        (r"$\alpha$", "α"),
        (r"$\alpha$-D-Glc", "α-D-Glc"),
        (r"$\beta_{D}$", "βᴅ"), // D may stay ASCII if no unicode map — check below
        ("**R**", "R"),
        ("OH", "OH"),
        ("NH2", "NH₂"),
        ("NH4+", "NH₄⁺"),
        ("O-", "O⁻"),
        ("*", "*"),
    ];

    for &(raw, want) in cases {
        let got = compose_label(&split_label(raw), LabelSide::East);
        // β_D: subscript D has no unicode subscript letter → stays 'D'.
        if raw == r"$\beta_{D}$" {
            assert_eq!(got, "βD", "raw={raw}");
            continue;
        }
        assert_eq!(got, want, "raw={raw}");
    }

    // West flips H-traveling labels only.
    assert_eq!(compose_label(&split_label("OH"), LabelSide::West), "HO");
    assert_eq!(compose_label(&split_label("NH2"), LabelSide::West), "H₂N");
    assert_eq!(compose_label(&split_label("$R_1$"), LabelSide::West), "R₁");
    assert_eq!(compose_label(&split_label("GlcA"), LabelSide::West), "GlcA");
}

/// Placed labels must produce drawable SVG paths (not empty / not panic).
#[test]
fn place_label_emits_ink_for_common_labels() {
    let samples = [
        "OH",
        "NH2",
        "NH4+",
        "$R_1$",
        "R^2",
        r"$\alpha$-D-Glc",
        "**Et**",
        "*",
        "my_name",
    ];
    for raw in samples {
        for side in [
            LabelSide::East,
            LabelSide::West,
            LabelSide::North,
            LabelSide::South,
        ] {
            let pl = place_label(raw, 40.0, 30.0, side, FONT_PX, FaceStyle::Regular);
            assert!(
                !pl.path_d.is_empty() && pl.path_d.contains('M'),
                "{raw:?} {side:?}: empty path"
            );
            assert!(!pl.text.is_empty(), "{raw:?} {side:?}: empty data-text");
            assert_eq!(pl.raw, raw);
            assert_eq!(pl.atom_x, 40.0);
            assert_eq!(pl.atom_y, 30.0);
        }
    }
}

#[test]
fn bold_base_ors_with_markup_italic() {
    let g = parse_label_markup("*cis*", FaceStyle::Bold);
    assert!(
        g.iter().all(|x| x.face == FaceStyle::BoldItalic),
        "bold_labels + *italic* → BoldItalic, got {:?}",
        g.iter().map(|x| x.face).collect::<Vec<_>>()
    );
    let pl = place_label("*cis*", 0.0, 0.0, LabelSide::East, FONT_PX, FaceStyle::Bold);
    assert_eq!(pl.text, "cis");
    assert!(!pl.path_d.is_empty());
}

#[test]
fn escapes_round_trip_as_literals() {
    assert_eq!(chars(r"\_\*"), "_*");
    assert_eq!(chars(r"R\^2"), "R^2");
    assert!(roles(r"R\^2").iter().all(|(_, r)| *r == ScriptRole::Normal));
    assert_eq!(compose_label(&split_label(r"R\_1"), LabelSide::East), "R_1");
}

#[test]
fn style_cmd_aliases_and_nested_scripts() {
    for cmd in [r"\textbf{Et}", r"\mathbf{Et}", r"\bf{Et}"] {
        let g = parse_label_markup(cmd, FaceStyle::Regular);
        assert!(g.iter().all(|x| x.face == FaceStyle::Bold), "{cmd}");
        assert_eq!(g.iter().map(|x| x.ch).collect::<String>(), "Et");
    }
    for cmd in [r"\textit{cis}", r"\mathit{cis}", r"\emph{cis}", r"\it{cis}"] {
        let g = parse_label_markup(cmd, FaceStyle::Regular);
        assert!(g.iter().all(|x| x.face == FaceStyle::Italic), "{cmd}");
    }
    let nested = parse_label_markup(r"\textbf{R^2}", FaceStyle::Regular);
    assert_eq!(nested[0].ch, 'R');
    assert_eq!(nested[0].face, FaceStyle::Bold);
    assert_eq!(nested[0].role, ScriptRole::Normal);
    assert_eq!(nested[1].ch, '2');
    assert_eq!(nested[1].role, ScriptRole::Superscript);
    assert_eq!(nested[1].face, FaceStyle::Bold);
}

#[test]
fn bold_r_caret_compose_and_place() {
    assert_eq!(compose_label(&split_label("**R**^2"), LabelSide::East), "R²");
    let pl = place_label("**R**^2", 0.0, 0.0, LabelSide::East, FONT_PX, FaceStyle::Regular);
    assert_eq!(pl.text, "R²");
    assert!(!pl.path_d.is_empty());
}

#[test]
fn bold_face_changes_oh_outline_not_text() {
    let regular = place_label("OH", 10.0, 10.0, LabelSide::East, FONT_PX, FaceStyle::Regular);
    let bold = place_label("OH", 10.0, 10.0, LabelSide::East, FONT_PX, FaceStyle::Bold);
    assert_eq!(regular.text, bold.text);
    assert_ne!(
        regular.path_d, bold.path_d,
        "bold Liberation stem must change glyph path ink"
    );
}

#[test]
fn charge_outside_math_dollars() {
    let parts = split_label("$R$+");
    assert_eq!(parts.center, "$R$");
    assert_eq!(parts.charge, 1);
    assert_eq!(compose_label(&parts, LabelSide::East), "R⁺");
}

#[test]
fn backbone_places_bold_markup_and_glca() {
    use xpict_core::labels::{place_backbone, AtomIn, BondIn};

    let atoms = vec![
        AtomIn {
            x: 0.0,
            y: 0.0,
            label: None,
        },
        AtomIn {
            x: 30.0,
            y: 0.0,
            label: Some("**R**".into()),
        },
        AtomIn {
            x: 0.0,
            y: 30.0,
            label: Some("GlcA".into()),
        },
    ];
    let bonds = vec![
        BondIn { begin: 0, end: 1 },
        BondIn { begin: 0, end: 2 },
    ];
    let (ends, labels) = place_backbone(&atoms, &bonds, FONT_PX, FaceStyle::Regular);
    assert_eq!(ends.len(), 2);
    let r = labels[1].as_ref().expect("**R**");
    assert_eq!(r.text, "R");
    assert!(!r.path_d.is_empty());
    let glc = labels[2].as_ref().expect("GlcA");
    assert_eq!(glc.text, "GlcA");
    assert!(!glc.path_d.is_empty());
}
