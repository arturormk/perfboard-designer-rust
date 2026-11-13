//! Converts SVG page files into one PDF, exactly as the desktop export does.
//!
//! `cargo run -p perfboard-print --example svg_pages_to_pdf -- out.pdf page1.svg page2.svg`

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let output = args
        .next()
        .ok_or("usage: svg_pages_to_pdf OUT.pdf PAGE.svg...")?;
    let pages = args
        .map(std::fs::read_to_string)
        .collect::<Result<Vec<_>, _>>()?;
    let pdf = perfboard_print::svg_pages_to_pdf(&pages, &perfboard_print::font_options())?;
    std::fs::write(&output, pdf)?;
    println!("wrote {} page(s) to {output}", pages.len());
    Ok(())
}
