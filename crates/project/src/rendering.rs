use std::fmt::Write;

use perfboard_core::{
    ComponentDefinition, ComponentInstance, ComponentKind, PinDefinition, Rotation, Saddle, Side,
    WireColor,
};

use crate::ProjectDocument;

/// Distance from the outermost holes to the board edge, in pitches: one pitch
/// for the wire margin plus the pad radius, so the edge is tangent to a ring
/// of holes one pitch outside the grid. Matches `BOARD_MARGIN` in the frontend.
const BOARD_MARGIN: f64 = 1.36;

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
        r#"<svg xmlns="http://www.w3.org/2000/svg" role="img" aria-label="{}, {:?} side" viewBox="-2 -2 {width} {height}"><style>.board{{fill:#352817;stroke:#d8b56d;stroke-width:.15}}.hole{{fill:#111;stroke:#d8b56d;stroke-width:.05}}.component{{fill:#30536b;stroke:#bde4ff;stroke-width:.12}}.bridge{{stroke:#d98945;stroke-width:.22;stroke-linecap:round}}.lead{{stroke:#a3a7ab;stroke-width:.12}}.shunt{{fill:#1b1b1b;stroke:#e0c060;stroke-width:.08}}.pin{{fill:#d4b25f;stroke:#111;stroke-width:.06}}.wire{{fill:none;stroke-width:.24;stroke-linecap:round;stroke-linejoin:round}}.wire-casing{{fill:none;stroke:#172019;stroke-width:.38;stroke-linecap:round;stroke-linejoin:round}}.saddle{{stroke-width:.3;stroke-linecap:round}}.saddle-leg{{stroke:#d8b56d;stroke-width:.06}}.part-lead{{stroke:#bde4ff;stroke-width:.08}}.polarity-band{{fill:#c9ccd1}}.polarity-plus{{fill:none;stroke:#e9ebee;stroke-width:.09;stroke-linecap:round}}.opposite{{opacity:.25;stroke-dasharray:.3 .2}}.label{{fill:#fff;font: .55px sans-serif;paint-order:stroke;stroke:#111;stroke-width:.08}}</style>"#,
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
        r#"<rect class="board" x="-{BOARD_MARGIN}" y="-{BOARD_MARGIN}" width="{}" height="{}" rx=".45"/><g transform="{}">"#,
        f64::from(board.size.intervals_x()) + BOARD_MARGIN * 2.0,
        f64::from(board.size.intervals_y()) + BOARD_MARGIN * 2.0,
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
            r#"<polyline class="wire-casing{opposite}" points="{points}"/><polyline class="wire{opposite}" data-wire-id="{}" points="{points}" stroke="{}"/>"#,
            wire.id,
            escape(wire_color(&wire.color))
        );
    }
    for rail in document.design.rails.values() {
        let points = rail
            .vertices
            .iter()
            .map(|point| format!("{},{}", half_grid(point.x2), half_grid(point.y2)))
            .collect::<Vec<_>>()
            .join(" ");
        let opposite = if rail.side == side { "" } else { " opposite" };
        let _ = write!(
            svg,
            r#"<polyline class="wire-casing{opposite}" points="{points}"/><polyline class="wire rail{opposite}" data-rail-id="{}" points="{points}" stroke="{}"/>"#,
            rail.id,
            escape(wire_color(&rail.color))
        );
    }
    for saddle in document.design.saddles.values() {
        render_saddle(&mut svg, saddle, side);
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
            f64::from(board.size.intervals_x()) - f64::from(instance.position.x)
        } else {
            f64::from(instance.position.x)
        };
        let _ = write!(
            svg,
            r#"<text class="label" x="{}" y="{}">{}</text>"#,
            x + 0.4,
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
        r#"<g data-component-id="{}" transform="translate({} {}) rotate({degrees})">"#,
        instance.id, instance.position.x, instance.position.y,
    );
    if definition.kind == ComponentKind::Bridge {
        let start = &definition.pins[0];
        let end = &definition.pins[1];
        let _ = write!(
            svg,
            r#"<line class="bridge{}" x1="{}" y1="{}" x2="{}" y2="{}"/>"#,
            if instance.component_side { " lead" } else { "" },
            start.x,
            start.y,
            end.x,
            end.y,
        );
    } else {
        for (x1, y1, x2, y2) in lead_segments(definition, pitch, PIN_RADIUS) {
            let _ = write!(
                svg,
                r#"<line class="part-lead" x1="{x1:.3}" y1="{y1:.3}" x2="{x2:.3}" y2="{y2:.3}"/>"#
            );
        }
        let _ = write!(
            svg,
            r#"<rect class="component" x="{}" y="{}" width="{}" height="{}" rx=".3"/>"#,
            definition.body.x / pitch,
            definition.body.y / pitch,
            definition.body.width / pitch,
            definition.body.height / pitch,
        );
        render_polarity(svg, definition, pitch);
    }
    let shunt_pins = (definition.kind == ComponentKind::Jumper && instance.shunted)
        .then_some(definition.pins.as_slice());
    if let Some([first, second]) = shunt_pins {
        let (x, y) = (first.x.min(second.x), first.y.min(second.y));
        let (width, height) = (first.x.abs_diff(second.x), first.y.abs_diff(second.y));
        let _ = write!(
            svg,
            r#"<rect class="shunt" x="{}" y="{}" width="{}" height="{}" rx=".15"/>"#,
            f64::from(x) - 0.4,
            f64::from(y) - 0.4,
            f64::from(width) + 0.8,
            f64::from(height) + 0.8,
        );
    }
    for pin in &definition.pins {
        let _ = write!(
            svg,
            r#"<circle class="pin" data-pin-id="{}" cx="{}" cy="{}" r="{PIN_RADIUS}"><title>{}</title></circle>"#,
            escape(&pin.id),
            pin.x,
            pin.y,
            escape(&format!("Pin {}: {}", pin.number, instance.pin_name(pin)))
        );
    }
    svg.push_str("</g>");
}

/// A pin's drawn radius, in holes.
const PIN_RADIUS: f64 = 0.25;

/// Leads from a short body's ends out to the rim of each pin (`pad_radius`
/// around it), as `(x1, y1, x2, y2)` in part-local holes, for a two-pin part
/// with its pins on one axis. A pin the body reaches gets none. Mirrors
/// `leadSegments` in `frontend/src/model/leads.ts`.
fn lead_segments(
    definition: &ComponentDefinition,
    pitch: f64,
    pad_radius: f64,
) -> Vec<(f64, f64, f64, f64)> {
    let [a, b] = definition.pins.as_slice() else {
        return Vec::new();
    };
    let along_x = a.y == b.y && a.x != b.x;
    let along_y = a.x == b.x && a.y != b.y;
    if !along_x && !along_y {
        return Vec::new();
    }
    let body = &definition.body;
    let (start, length) = if along_x {
        (body.x, body.width)
    } else {
        (body.y, body.height)
    };
    let (start, end) = (start / pitch, (start + length) / pitch);
    [a, b]
        .into_iter()
        .filter_map(|pin| {
            let (at, across) = if along_x {
                (f64::from(pin.x), f64::from(pin.y))
            } else {
                (f64::from(pin.y), f64::from(pin.x))
            };
            let (from, to) = if at + pad_radius < start {
                (start, at + pad_radius)
            } else if at - pad_radius > end {
                (end, at - pad_radius)
            } else {
                return None;
            };
            Some(if along_x {
                (from, across, to, across)
            } else {
                (across, from, across, to)
            })
        })
        .collect()
}

/// A polarity mark on a part's body, in part-local holes.
#[derive(Debug, PartialEq)]
enum PolarityMark {
    /// A diode's cathode band.
    Band {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    /// An electrolytic capacitor's plus sign, centred on (`x`, `y`).
    Plus { x: f64, y: f64, arm: f64 },
}

/// The polarity mark read from a part's pin IDs: a cathode band for pins
/// `cathode` and `anode`, a plus sign by the positive lead for pins
/// `positive` and `negative`. Mirrors `polarityMark` in
/// `frontend/src/model/polarity.ts`.
fn polarity_mark(definition: &ComponentDefinition, pitch: f64) -> Option<PolarityMark> {
    let pin = |id: &str| definition.pins.iter().find(|pin| pin.id == id);
    let body = &definition.body;
    let (bx, by, bw, bh) = (
        body.x / pitch,
        body.y / pitch,
        body.width / pitch,
        body.height / pitch,
    );
    // Leads side by side along x (true), along y (false), or neither.
    let axis = |a: &PinDefinition, b: &PinDefinition| {
        if a.y == b.y && a.x != b.x {
            Some(true)
        } else if a.x == b.x && a.y != b.y {
            Some(false)
        } else {
            None
        }
    };
    if let (Some(cathode), Some(anode)) = (pin("cathode"), pin("anode")) {
        let along_x = axis(cathode, anode)?;
        let (start, length) = if along_x { (bx, bw) } else { (by, bh) };
        let toward_start = if along_x {
            cathode.x < anode.x
        } else {
            cathode.y < anode.y
        };
        let width = length * 0.16;
        let inset = length * 0.1;
        let from = if toward_start {
            start + inset
        } else {
            start + length - inset - width
        };
        return Some(if along_x {
            PolarityMark::Band {
                x: from,
                y: by,
                width,
                height: bh,
            }
        } else {
            PolarityMark::Band {
                x: bx,
                y: from,
                width: bw,
                height: width,
            }
        });
    }
    if let (Some(positive), Some(negative)) = (pin("positive"), pin("negative")) {
        let along_x = axis(positive, negative)?;
        let half = if along_x { bh } else { bw } / 2.0;
        let across = if along_x { by } else { bx } + half * 0.4;
        let arm = (half * 0.22).min(0.2);
        return Some(if along_x {
            PolarityMark::Plus {
                x: f64::from(positive.x),
                y: across,
                arm,
            }
        } else {
            PolarityMark::Plus {
                x: across,
                y: f64::from(positive.y),
                arm,
            }
        });
    }
    None
}

fn render_polarity(svg: &mut String, definition: &ComponentDefinition, pitch: f64) {
    match polarity_mark(definition, pitch) {
        Some(PolarityMark::Band {
            x,
            y,
            width,
            height,
        }) => {
            let _ = write!(
                svg,
                r#"<rect class="polarity-band" x="{x:.3}" y="{y:.3}" width="{width:.3}" height="{height:.3}"><title>Cathode band</title></rect>"#
            );
        }
        Some(PolarityMark::Plus { x, y, arm }) => {
            let _ = write!(
                svg,
                r#"<path class="polarity-plus" d="M {:.3} {y:.3} H {:.3} M {x:.3} {:.3} V {:.3}"><title>Positive lead</title></path>"#,
                x - arm,
                x + arm,
                y - arm,
                y + arm,
            );
        }
        None => {}
    }
}

fn render_saddle(svg: &mut String, saddle: &Saddle, side: Side) {
    let [start, end] = saddle.vertices();
    let opposite = if saddle.side == side { "" } else { " opposite" };
    let (x1, y1, x2, y2) = (
        half_grid(start.x2),
        half_grid(start.y2),
        half_grid(end.x2),
        half_grid(end.y2),
    );
    let _ = write!(
        svg,
        r#"<g class="saddle-group{opposite}" data-saddle-id="{id}"><line class="saddle" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{color}"/><circle class="saddle-leg" cx="{x1}" cy="{y1}" r=".16" fill="{color}"/><circle class="saddle-leg" cx="{x2}" cy="{y2}" r=".16" fill="{color}"/></g>"#,
        id = saddle.id,
        color = escape(wire_color(&saddle.color)),
    );
}

fn wire_color(color: &WireColor) -> &str {
    match color {
        WireColor::Green => "green",
        WireColor::Red => "red",
        WireColor::Black => "black",
        WireColor::Yellow => "yellow",
        WireColor::White => "#e6e6e6",
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

    fn part(file: &str) -> ComponentDefinition {
        let path = format!(
            "{}/../../components/{file}/component.toml",
            env!("CARGO_MANIFEST_DIR")
        );
        ComponentDefinition::from_toml(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn diodes_carry_a_cathode_band_at_the_cathode_end() {
        let diode = part("diode-do-41");
        let Some(PolarityMark::Band { x, width, .. }) = polarity_mark(&diode, 2.54) else {
            panic!("no cathode band");
        };
        // The cathode lead is at x = 0, the anode at x = 4: the band sits in
        // the body's cathode half.
        let body_x = diode.body.x / 2.54;
        let body_end = body_x + diode.body.width / 2.54;
        assert!(x > body_x && x + width < f64::midpoint(body_x, body_end));
        assert!(matches!(
            polarity_mark(&part("diode-do-35"), 2.54),
            Some(PolarityMark::Band { .. })
        ));
    }

    #[test]
    fn electrolytics_carry_a_plus_by_the_positive_lead() {
        let capacitor = part("capacitor-electrolytic-5mm");
        let Some(PolarityMark::Plus { x, y, .. }) = polarity_mark(&capacitor, 2.54) else {
            panic!("no plus sign");
        };
        assert!((x - 0.0).abs() < f64::EPSILON);
        assert!(y < -0.36 && y > capacitor.body.y / 2.54);
    }

    #[test]
    fn short_bodies_are_joined_to_their_pins() {
        let diode = part("diode-do-41");
        let leads = lead_segments(&diode, 2.54, 0.25);
        let body_x = diode.body.x / 2.54;
        let body_end = body_x + diode.body.width / 2.54;
        assert_eq!(leads.len(), 2);
        assert_eq!(leads[0], (body_x, 0.0, 0.25, 0.0));
        assert_eq!(leads[1], (body_end, 0.0, 3.75, 0.0));
        assert_eq!(lead_segments(&part("dip-8"), 2.54, 0.25), []);
    }

    #[test]
    fn unpolarized_parts_carry_no_mark() {
        assert_eq!(polarity_mark(&part("capacitor-ceramic-2-54mm"), 2.54), None);
        assert_eq!(polarity_mark(&part("dip-8"), 2.54), None);
    }

    #[test]
    fn white_wires_render_as_a_subtle_gray() {
        assert_eq!(wire_color(&WireColor::White), "#e6e6e6");
    }
}
