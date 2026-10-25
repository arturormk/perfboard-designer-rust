use std::fmt::Write;

use perfboard_core::{ComponentDefinition, ComponentInstance, Rotation, Side, WireColor};

use crate::ProjectDocument;

/// Renders a deterministic, self-contained overview of the authoritative design.
#[must_use]
pub fn render_design_svg(document: &ProjectDocument, side: Side) -> String {
    let board = &document.design.board;
    let padding = 2.0;
    let mut svg = String::new();
    let width = f64::from(board.size.intervals_x()) + padding * 2.0;
    let height = f64::from(board.size.intervals_y()) + padding * 2.0;
    let _ = write!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" role="img" aria-label="{}, {:?} side" viewBox="-2 -2 {width} {height}"><style>.board{{fill:#352817;stroke:#d8b56d;stroke-width:.15}}.hole{{fill:#111;stroke:#d8b56d;stroke-width:.05}}.component{{fill:#30536b;stroke:#bde4ff;stroke-width:.12}}.pin{{fill:#d4b25f;stroke:#111;stroke-width:.06}}.wire{{fill:none;stroke-width:.24;stroke-linecap:round;stroke-linejoin:round}}.opposite{{opacity:.25;stroke-dasharray:.3 .2}}.label{{fill:#fff;font: .55px sans-serif;paint-order:stroke;stroke:#111;stroke-width:.08}}</style>"#,
        escape(&board.name),
        side
    );
    let mirror = if side == Side::Solder {
        format!("translate({} 0) scale(-1 1)", board.size.intervals_x())
    } else {
        String::new()
    };
    let _ = write!(
        svg,
        r#"<rect class="board" x="-.5" y="-.5" width="{}" height="{}" rx=".5"/><g transform="{}">"#,
        board.size.holes_x(),
        board.size.holes_y(),
        mirror
    );
    for y in 0..board.size.holes_y() {
        for x in 0..board.size.holes_x() {
            let _ = write!(svg, r#"<circle class="hole" cx="{x}" cy="{y}" r=".12"/>"#);
        }
    }
    for wire in document.design.wires.values() {
        let points = wire
            .vertices
            .iter()
            .map(|point| format!("{},{}", half_grid(point.x2), half_grid(point.y2)))
            .collect::<Vec<_>>()
            .join(" ");
        let opposite = if wire.side == side { "" } else { " opposite" };
        let _ = write!(
            svg,
            r#"<polyline class="wire{opposite}" data-wire-id="{}" points="{points}" stroke="{}"/>"#,
            wire.id,
            escape(wire_color(&wire.color))
        );
    }
    for instance in document.design.components.values() {
        if let Some(definition) = document
            .design
            .component_definitions
            .get(&instance.definition_id)
        {
            render_component(&mut svg, instance, definition, board.pitch_mm);
        }
    }
    svg.push_str("</g>");
    for instance in document.design.components.values() {
        let x = if side == Side::Solder {
            board.size.intervals_x() - instance.position.x
        } else {
            instance.position.x
        };
        let _ = write!(
            svg,
            r#"<text class="label" x="{}" y="{}">{}</text>"#,
            f64::from(x) + 0.4,
            f64::from(instance.position.y) + 1.2,
            escape(&instance.name)
        );
    }
    svg.push_str("</svg>");
    svg
}

fn half_grid(value: i64) -> String {
    let magnitude = value.unsigned_abs();
    let sign = if value.is_negative() { "-" } else { "" };
    if magnitude.is_multiple_of(2) {
        format!("{sign}{}", magnitude / 2)
    } else {
        format!("{sign}{}.5", magnitude / 2)
    }
}

fn render_component(
    svg: &mut String,
    instance: &ComponentInstance,
    definition: &ComponentDefinition,
    pitch: f64,
) {
    let degrees = match instance.rotation {
        Rotation::Deg0 => 0,
        Rotation::Deg90 => 90,
        Rotation::Deg180 => 180,
        Rotation::Deg270 => 270,
    };
    let _ = write!(
        svg,
        r#"<g data-component-id="{}" transform="translate({} {}) rotate({degrees})"><rect class="component" x="{}" y="{}" width="{}" height="{}" rx=".3"/>"#,
        instance.id,
        instance.position.x,
        instance.position.y,
        definition.body.x / pitch,
        definition.body.y / pitch,
        definition.body.width / pitch,
        definition.body.height / pitch,
    );
    for pin in &definition.pins {
        let _ = write!(
            svg,
            r#"<circle class="pin" data-pin-id="{}" cx="{}" cy="{}" r=".25"><title>{}</title></circle>"#,
            escape(&pin.id),
            pin.x,
            pin.y,
            escape(&format!("Pin {}: {}", pin.number, pin.name))
        );
    }
    svg.push_str("</g>");
}

fn wire_color(color: &WireColor) -> &str {
    match color {
        WireColor::Green => "green",
        WireColor::Red => "red",
        WireColor::Black => "black",
        WireColor::Yellow => "yellow",
        WireColor::White => "white",
        WireColor::Blue => "blue",
        WireColor::Orange => "orange",
        WireColor::Other(value) => value,
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use perfboard_core::{BoardSize, ProjectId};

    use super::*;
    use crate::Board;

    #[test]
    fn rendering_is_deterministic_and_escaped() {
        let document = ProjectDocument::new(
            ProjectId::new(),
            "render",
            Board::new("<board>", BoardSize::new(2, 2).unwrap(), 2.54),
        );
        let first = render_design_svg(&document, Side::Component);
        assert_eq!(first, render_design_svg(&document, Side::Component));
        assert!(first.contains("&lt;board&gt;"));
        assert!(first.contains("viewBox=\"-2 -2 5 5\""));
    }

    #[test]
    fn solder_view_mirrors_geometry() {
        let document = ProjectDocument::new(
            ProjectId::new(),
            "render",
            Board::new("board", BoardSize::new(3, 2).unwrap(), 2.54),
        );
        assert!(render_design_svg(&document, Side::Solder).contains("translate(2 0) scale(-1 1)"));
    }
}
