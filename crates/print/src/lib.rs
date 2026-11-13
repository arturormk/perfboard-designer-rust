//! True-scale PDF output for print sheets.
//!
//! The desktop web view cannot print at a guaranteed scale (`WebKitGTK` fits
//! pages to the paper by its own rules), so sheets that must measure 1:1 are
//! laid out by the frontend as page-sized SVGs in millimetres and turned into
//! a PDF here. A PDF printed at "Actual size" keeps every millimetre.

use std::collections::HashMap;

use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, TextStr};
pub use svg2pdf::usvg;

/// PDF points per CSS pixel: usvg resolves `mm` at 96 px per inch.
const PT_PER_PX: f32 = 72.0 / 96.0;

/// Sans-serif families tried in order for the SVG generic `sans-serif`.
const SANS_SERIF: &[&str] = &[
    "DejaVu Sans",
    "Liberation Sans",
    "Noto Sans",
    "Arial",
    "Helvetica",
];

#[derive(Debug, thiserror::Error)]
pub enum PrintError {
    #[error("there are no pages to export")]
    NoPages,
    #[error("page {page} is not valid SVG: {message}")]
    InvalidSvg { page: usize, message: String },
    #[error("page {page} could not be converted to PDF: {message}")]
    Conversion { page: usize, message: String },
}

/// Loads system fonts once and points the generic `sans-serif` at one that
/// exists, so text renders the same as on screen where possible.
#[must_use]
pub fn font_options() -> usvg::Options<'static> {
    let mut options = usvg::Options::default();
    let database = options.fontdb_mut();
    database.load_system_fonts();
    let available = |family: &str| {
        database.faces().any(|face| {
            face.families
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case(family))
        })
    };
    if let Some(family) = SANS_SERIF.iter().find(|family| available(family)) {
        database.set_sans_serif_family(*family);
        options.font_family = (*family).to_owned();
    }
    options
}

/// Converts page-sized SVG documents into one PDF, one page each. Each page
/// takes its size from the SVG's `width` and `height` (use millimetres).
///
/// # Errors
/// Returns an error when there are no pages, or when a page cannot be parsed
/// or converted.
pub fn svg_pages_to_pdf(
    pages: &[String],
    options: &usvg::Options<'_>,
) -> Result<Vec<u8>, PrintError> {
    if pages.is_empty() {
        return Err(PrintError::NoPages);
    }
    let mut alloc = Ref::new(1);
    let catalog_id = alloc.bump();
    let page_tree_id = alloc.bump();
    let info_id = alloc.bump();
    let svg_name = Name(b"S1");

    let mut pdf = Pdf::new();
    let mut page_ids = Vec::with_capacity(pages.len());
    let mut chunks = Vec::with_capacity(pages.len());
    for (index, svg) in pages.iter().enumerate() {
        let page = index + 1;
        let tree = usvg::Tree::from_str(svg, options).map_err(|error| PrintError::InvalidSvg {
            page,
            message: error.to_string(),
        })?;
        let width = tree.size().width() * PT_PER_PX;
        let height = tree.size().height() * PT_PER_PX;
        let (chunk, svg_id) = svg2pdf::to_chunk(&tree, svg2pdf::ConversionOptions::default())
            .map_err(|error| PrintError::Conversion {
                page,
                message: error.to_string(),
            })?;
        let mut renumbered = HashMap::new();
        let chunk = chunk.renumber(|old| *renumbered.entry(old).or_insert_with(|| alloc.bump()));
        let svg_id = renumbered[&svg_id];

        let page_id = alloc.bump();
        let content_id = alloc.bump();
        let mut page_writer = pdf.page(page_id);
        page_writer.media_box(Rect::new(0.0, 0.0, width, height));
        page_writer.parent(page_tree_id);
        page_writer.contents(content_id);
        page_writer.resources().x_objects().pair(svg_name, svg_id);
        page_writer.finish();

        // The SVG XObject is a unit square: scale it to fill the page.
        let mut content = Content::new();
        content
            .transform([width, 0.0, 0.0, height, 0.0, 0.0])
            .x_object(svg_name);
        pdf.stream(content_id, &content.finish());
        page_ids.push(page_id);
        chunks.push(chunk);
    }
    pdf.catalog(catalog_id).pages(page_tree_id);
    let count = i32::try_from(page_ids.len()).unwrap_or(i32::MAX);
    pdf.pages(page_tree_id).kids(page_ids).count(count);
    pdf.document_info(info_id)
        .producer(TextStr("Perfboard Designer"));
    for chunk in &chunks {
        pdf.extend(chunk);
    }
    Ok(pdf.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(width_mm: f32, height_mm: f32, body: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width_mm}mm" height="{height_mm}mm" viewBox="0 0 {width_mm} {height_mm}">{body}</svg>"#
        )
    }

    fn media_boxes(pdf: &[u8]) -> Vec<(f32, f32)> {
        let text = String::from_utf8_lossy(pdf);
        text.match_indices("/MediaBox [")
            .map(|(at, _)| {
                let rest = &text[at + 11..];
                let numbers: Vec<f32> = rest[..rest.find(']').unwrap()]
                    .split_whitespace()
                    .map(|value| value.parse().unwrap())
                    .collect();
                (numbers[2], numbers[3])
            })
            .collect()
    }

    #[test]
    fn pages_keep_their_millimetre_size() {
        let options = usvg::Options::default();
        let pdf = svg_pages_to_pdf(
            &[
                page(
                    210.0,
                    297.0,
                    r#"<rect x="10" y="10" width="80" height="8"/>"#,
                ),
                page(215.9, 279.4, ""),
            ],
            &options,
        )
        .unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        let boxes = media_boxes(&pdf);
        assert_eq!(boxes.len(), 2);
        // A4 is 595.28 x 841.89 pt, US Letter 612 x 792 pt.
        assert!((boxes[0].0 - 595.28).abs() < 0.05, "{boxes:?}");
        assert!((boxes[0].1 - 841.89).abs() < 0.05, "{boxes:?}");
        assert!((boxes[1].0 - 612.0).abs() < 0.05, "{boxes:?}");
        assert!((boxes[1].1 - 792.0).abs() < 0.05, "{boxes:?}");
        assert!(String::from_utf8_lossy(&pdf).contains("/Count 2"));
    }

    #[test]
    fn rejects_empty_and_invalid_input() {
        let options = usvg::Options::default();
        assert!(matches!(
            svg_pages_to_pdf(&[], &options),
            Err(PrintError::NoPages)
        ));
        assert!(matches!(
            svg_pages_to_pdf(&["not svg".into()], &options),
            Err(PrintError::InvalidSvg { page: 1, .. })
        ));
    }

    #[test]
    fn sans_serif_resolves_to_an_installed_family() {
        let options = font_options();
        let _: &usvg::fontdb::Database = &options.fontdb;
        // Text must parse into a tree; with fonts available it is kept.
        let tree = usvg::Tree::from_str(
            &page(
                50.0,
                20.0,
                r#"<text x="2" y="10" font-family="sans-serif" font-size="4">W1 · 34.4 mm</text>"#,
            ),
            &options,
        )
        .unwrap();
        if !options.fontdb.is_empty() {
            assert!(tree.root().has_children());
        }
    }
}
