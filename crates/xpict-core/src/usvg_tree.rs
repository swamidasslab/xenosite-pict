//! Shared SVG parse for PDF / PNG export, with the bundled Liberation Sans as
//! the only font so `<text>` renders the same on every machine.

use std::sync::{Arc, OnceLock};

use usvg::fontdb::Database;

fn fontdb() -> Arc<Database> {
    static DB: OnceLock<Arc<Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = Database::new();
        for face in [
            crate::font::FONT_REGULAR,
            crate::font::FONT_BOLD,
            crate::font::FONT_ITALIC,
            crate::font::FONT_BOLD_ITALIC,
        ] {
            db.load_font_data(face.to_vec());
        }
        db.set_sans_serif_family("Liberation Sans");
        Arc::new(db)
    })
    .clone()
}

pub(crate) fn parse(svg: &str) -> Result<usvg::Tree, String> {
    let opts = usvg::Options {
        font_family: "Liberation Sans".into(),
        fontdb: fontdb(),
        ..usvg::Options::default()
    };
    usvg::Tree::from_str(svg, &opts).map_err(|e| e.to_string())
}
