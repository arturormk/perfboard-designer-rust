//! Breadboard layout: the design rebuilt on standard solderless breadboards,
//! so a circuit can be tried on the bench before anything is soldered
//! (ADR 0018).
//!
//! Like the assembly plan, the layout is derived data: a pure function of the
//! design, recomputed on request and never stored. The same design always
//! yields the same layout.
//!
//! # The board
//!
//! A standard board has 63 rows. Each row has two strips of five connected
//! holes (columns a–e and f–j) either side of a central channel two holes
//! wide, so a 0.3" DIP straddles it. Down each side run a − and a + rail,
//! connected from row 3 to row 61; their holes come in groups of five
//! (rows 3–7, 9–13, …), with one row left out between groups. Boards are joined side by side when one is
//! not tall enough.
//!
//! In layout coordinates one hole is one unit, rows run down (y) and boards
//! sit left to right (x). Within a board, columns are:
//!
//! ```text
//!  0  1  2  3 4 5 6 7  8 9  10 11 12 13 14  15  16 17
//!  +  −     a b c d e  ···   f  g  h  i  j       +  −
//! ```
//!
//! # Strategy
//!
//! Readability over density, as on a real bench: every part gets its own
//! rows with a free row above and below, and no two parts share a strip.
//! Parts with two pin columns 3–11 holes apart straddle the channel; parts
//! whose pins can each take their own row go on one side; anything else (a
//! 2×N header, a wide module) sits beside the boards and is wired in with
//! male–female Dupont leads. Each net is then joined with jumpers: the four
//! rails carry the busiest nets, ground and supply first, and other nets are
//! joined strip to strip along a shortest spanning tree. Every jumper takes
//! the colour of the net's wires on the perfboard.
//!
//! [`verify_breadboard_layout`] rebuilds the copper connectivity of a layout
//! and checks it against the design's nets; the layout runs it on itself and
//! reports any mismatch as a warning.

use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    ComponentDefinition, ComponentInstance, ComponentInstanceId, ComponentKind, ConnectionState,
    HolePos, NetId, ObjectReference, PinRef, Rotation, ValueReading, WireColor, read_value,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Design, DocumentRevision};

/// Rows on one board.
pub const BREADBOARD_ROWS: u32 = 63;
/// First rail row, 0-based (row 3).
pub const RAIL_FIRST_ROW: u32 = 2;
/// Last rail row, 0-based (row 61).
pub const RAIL_LAST_ROW: u32 = 60;
/// Rail holes come in groups of five with one row left out between groups:
/// rows 3–7, 9–13, … 57–61. Every hole of a rail is still connected.
pub const RAIL_GROUP: u32 = 5;

/// Whether a 0-based row has rail holes.
#[must_use]
pub const fn is_rail_row(row: u32) -> bool {
    row >= RAIL_FIRST_ROW
        && row <= RAIL_LAST_ROW
        && (row - RAIL_FIRST_ROW) % (RAIL_GROUP + 1) < RAIL_GROUP
}

/// Every 0-based row with rail holes, top to bottom.
fn rail_rows() -> impl DoubleEndedIterator<Item = u32> {
    (RAIL_FIRST_ROW..=RAIL_LAST_ROW).filter(|row| is_rail_row(*row))
}
/// Columns of one board, rails included.
pub const BOARD_COLUMNS: u32 = 18;
/// Distance from one board's first column to the next board's.
pub const BOARD_STRIDE: u32 = BOARD_COLUMNS + 1;
/// Columns a–e and f–j within a board.
pub const LEFT_COLUMNS: [u32; 5] = [3, 4, 5, 6, 7];
pub const RIGHT_COLUMNS: [u32; 5] = [10, 11, 12, 13, 14];
/// Free columns between the last board and the off-board parts.
const OFF_BOARD_GAP: u32 = 3;
/// Free rows between stacked off-board parts.
const OFF_BOARD_SPACING: i64 = 2;
/// The channel lies between columns e and f: a straddling part's pin columns
/// may be 3 (e–f) to 11 (a–j) holes apart.
const MIN_STRADDLE: i64 = 3;
const MAX_STRADDLE: i64 = 11;

// ── Output ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BreadboardLayout {
    /// The working-copy revision the layout was derived from.
    pub source_revision: DocumentRevision,
    pub board_count: u32,
    pub geometry: BreadboardGeometry,
    pub parts: Vec<BreadboardPart>,
    /// Rails that carry a net, per board.
    pub rails: Vec<BreadboardRail>,
    pub jumpers: Vec<BreadboardJumper>,
    pub warnings: Vec<BreadboardWarning>,
}

/// Board dimensions, so a viewer draws exactly what the layout assumes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BreadboardGeometry {
    pub rows: u32,
    pub rail_first_row: u32,
    pub rail_last_row: u32,
    /// Every 0-based row with rail holes: groups of five, one row left out
    /// between groups.
    pub rail_rows: Vec<u32>,
    pub board_columns: u32,
    pub board_stride: u32,
    pub left_columns: Vec<u32>,
    pub right_columns: Vec<u32>,
    pub rail_columns: Vec<RailColumn>,
    /// First column of the area beside the boards for parts that cannot sit
    /// on them. Equal to `width` when there are none.
    pub off_board_x: u32,
    /// Extent of everything laid out, in holes.
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RailColumn {
    pub slot: RailSlot,
    pub column: u32,
    pub polarity: RailPolarity,
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RailSlot {
    LeftOuter,
    LeftInner,
    RightInner,
    RightOuter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RailPolarity {
    Minus,
    Plus,
}

impl RailSlot {
    pub const ALL: [Self; 4] = [
        Self::LeftOuter,
        Self::LeftInner,
        Self::RightInner,
        Self::RightOuter,
    ];

    #[must_use]
    pub const fn column(self) -> u32 {
        match self {
            Self::LeftOuter => 0,
            Self::LeftInner => 1,
            Self::RightInner => 16,
            Self::RightOuter => 17,
        }
    }

    /// Each pair reads + then − from left to right, on both sides of the
    /// board: red +, blue − … red +, blue −.
    #[must_use]
    pub const fn polarity(self) -> RailPolarity {
        match self {
            Self::LeftOuter | Self::RightInner => RailPolarity::Plus,
            Self::LeftInner | Self::RightOuter => RailPolarity::Minus,
        }
    }

    const fn lane(self) -> Lane {
        match self {
            Self::LeftOuter | Self::LeftInner => Lane::Left,
            Self::RightInner | Self::RightOuter => Lane::Right,
        }
    }

    fn from_column(column: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|slot| slot.column() == column)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PartPlacement {
    /// Across the central channel, one pin column on each side.
    Straddle,
    /// On columns a–e.
    Left,
    /// On columns f–j.
    Right,
    /// Beside the boards, wired in with Dupont leads.
    OffBoard,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BreadboardPart {
    pub component_id: ComponentInstanceId,
    pub name: String,
    /// The part's value, with its colour code or marking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<ValueReading>,
    pub definition_id: String,
    /// The component's origin in layout coordinates.
    pub position: HolePos,
    pub rotation: Rotation,
    pub placement: PartPlacement,
    /// The board it sits on; `None` off the boards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BreadboardRail {
    pub board: u32,
    pub slot: RailSlot,
    pub net_id: NetId,
    pub net_name: String,
    pub color: WireColor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JumperKind {
    /// Male–male Dupont jumper between two board holes.
    Jumper,
    /// Male–female Dupont lead from an off-board part's pin to a board hole.
    Lead,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BreadboardJumper {
    /// Stable within one layout: `jumper:<n>`.
    pub id: String,
    /// 1-based number, as shown on the board.
    pub number: u32,
    pub kind: JumperKind,
    pub net_id: NetId,
    pub net_name: String,
    pub color: WireColor,
    pub from: HolePos,
    pub to: HolePos,
    /// Readable ends, e.g. `e12`, `B2 left − rail, row 7`, `J1 · 2 VCC`.
    pub from_label: String,
    pub to_label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BreadboardWarning {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<ObjectReference>,
}

// ── Internals ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Lane {
    Left,
    Right,
}

impl Lane {
    const fn columns(self) -> [u32; 5] {
        match self {
            Self::Left => LEFT_COLUMNS,
            Self::Right => RIGHT_COLUMNS,
        }
    }

    /// Columns ordered from the board edge (rails) toward the channel.
    fn outer_first(self) -> [u32; 5] {
        let mut columns = self.columns();
        if self == Self::Right {
            columns.reverse();
        }
        columns
    }

    fn of_column(column: u32) -> Option<Self> {
        if LEFT_COLUMNS.contains(&column) {
            Some(Self::Left)
        } else if RIGHT_COLUMNS.contains(&column) {
            Some(Self::Right)
        } else {
            None
        }
    }
}

/// One five-hole strip: board, side, 0-based row.
type Strip = (u32, Lane, u32);

/// Holes are handled as signed points so footprints can be rotated and
/// shifted freely; everything laid out ends up non-negative.
type Point = (i64, i64);

fn hole(point: Point) -> HolePos {
    HolePos::new(
        i32::try_from(point.0.max(0)).unwrap_or(i32::MAX),
        i32::try_from(point.1.max(0)).unwrap_or(i32::MAX),
    )
}

/// A breadboard hole from its column and row; both are far below `i32::MAX`.
fn bb_hole(column: u32, row: u32) -> HolePos {
    HolePos::new(
        i32::try_from(column).unwrap_or(i32::MAX),
        i32::try_from(row).unwrap_or(i32::MAX),
    )
}

/// A laid-out hole's column and row. Layouts never go negative; a negative
/// coordinate maps past every board, so it matches no strip or rail.
fn bb_coords(hole: HolePos) -> (u32, u32) {
    (
        u32::try_from(hole.x).unwrap_or(u32::MAX),
        u32::try_from(hole.y).unwrap_or(u32::MAX),
    )
}

fn point(hole: HolePos) -> Point {
    (i64::from(hole.x), i64::from(hole.y))
}

fn strip_of(hole: HolePos) -> Option<Strip> {
    let (x, y) = bb_coords(hole);
    let board = x / BOARD_STRIDE;
    let lane = Lane::of_column(x % BOARD_STRIDE)?;
    (y < BREADBOARD_ROWS).then_some((board, lane, y))
}

fn rail_of(hole: HolePos) -> Option<(u32, RailSlot)> {
    let (x, y) = bb_coords(hole);
    let slot = RailSlot::from_column(x % BOARD_STRIDE)?;
    is_rail_row(y).then_some((x / BOARD_STRIDE, slot))
}

fn strip_hole(strip: Strip, column: u32) -> HolePos {
    bb_hole(strip.0 * BOARD_STRIDE + column, strip.2)
}

/// A strip's centre, for distances.
fn strip_center(strip: Strip) -> Point {
    let column = match strip.1 {
        Lane::Left => 5,
        Lane::Right => 12,
    };
    (
        i64::from(strip.0 * BOARD_STRIDE + column),
        i64::from(strip.2),
    )
}

const fn distance(a: Point, b: Point) -> i64 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

/// Holes a part takes in its own frame (unrotated): its pins, its declared
/// occupied holes, and every hole strictly under its body outline.
fn footprint_cover(definition: &ComponentDefinition, pitch_mm: f64) -> BTreeSet<Point> {
    let mut cover = BTreeSet::new();
    for pin in &definition.pins {
        cover.insert((i64::from(pin.x), i64::from(pin.y)));
    }
    for occupied in &definition.occupied_holes {
        cover.insert((i64::from(occupied.x), i64::from(occupied.y)));
    }
    let body = definition.body;
    if pitch_mm > 0.0 && body.width.is_finite() && body.height.is_finite() {
        let (x0, x1) = (body.x / pitch_mm, (body.x + body.width) / pitch_mm);
        let (y0, y1) = (body.y / pitch_mm, (body.y + body.height) / pitch_mm);
        let inside = |value: f64, low: f64, high: f64| value > low + 1e-6 && value < high - 1e-6;
        // Bodies are a few dozen holes at most; the bounds keep a malformed
        // one from looping for long.
        #[allow(clippy::cast_possible_truncation)] // Clamped to a small range first.
        let range = |low: f64, high: f64| {
            (low.floor().clamp(-200.0, 200.0) as i64)..=(high.ceil().clamp(-200.0, 200.0) as i64)
        };
        for x in range(x0, x1) {
            for y in range(y0, y1) {
                #[allow(clippy::cast_precision_loss)] // Small integers.
                let (fx, fy) = (x as f64, y as f64);
                if inside(fx, x0, x1) && inside(fy, y0, y1) {
                    cover.insert((x, y));
                }
            }
        }
    }
    cover
}

fn rotate(rotation: Rotation, point: Point) -> Point {
    // Footprint coordinates are small, so the i32 round trip is lossless.
    let x = i32::try_from(point.0).unwrap_or(0);
    let y = i32::try_from(point.1).unwrap_or(0);
    let (rx, ry) = rotation.rotate_local((x, y));
    (i64::from(rx), i64::from(ry))
}

/// Holes a part's required access regions span in its own frame (unrotated):
/// where a cable plugs in or a screw is turned, so nothing may go there. Its
/// own pins are left out.
fn access_cover(definition: &ComponentDefinition) -> BTreeSet<Point> {
    let pins: BTreeSet<Point> = definition
        .pins
        .iter()
        .map(|pin| (i64::from(pin.x), i64::from(pin.y)))
        .collect();
    let mut access = BTreeSet::new();
    for area in definition.access.iter().filter(|area| area.required) {
        let (x0, x1) = (area.min.x.min(area.max.x), area.min.x.max(area.max.x));
        let (y0, y1) = (area.min.y.min(area.max.y), area.min.y.max(area.max.y));
        for x in x0..=x1 {
            for y in y0..=y1 {
                let point = (i64::from(x), i64::from(y));
                if !pins.contains(&point) {
                    access.insert(point);
                }
            }
        }
    }
    access
}

/// The board holes under a part's required access regions, given its origin
/// and rotation. Parts of a region beyond the boards lie over open air and
/// need no reserving.
fn access_holes(
    definition: &ComponentDefinition,
    rotation: Rotation,
    origin: Point,
) -> Vec<HolePos> {
    access_cover(definition)
        .into_iter()
        .map(|p| rotate(rotation, p))
        .map(|(x, y)| (origin.0 + x, origin.1 + y))
        .filter(|&(x, y)| x >= 0 && y >= 0)
        .map(hole)
        .filter(|h| strip_of(*h).is_some() || rail_of(*h).is_some())
        .collect()
}

/// How one part can sit on a board: rotation, side, and where its origin lies
/// across the board. Rows are chosen later.
#[derive(Clone, Debug)]
struct Fit {
    rotation: Rotation,
    placement: PartPlacement,
    /// Board column of the part's origin.
    shift_x: i64,
    /// Rotated pins, relative to the origin.
    pins: Vec<(String, Point)>,
    /// Rotated cover, relative to the origin; includes the pins.
    cover: BTreeSet<Point>,
    /// Rotated required access regions, relative to the origin: kept clear.
    access: BTreeSet<Point>,
    top: i64,
    bottom: i64,
    /// (access regions face away from the channel, fewest free holes in any
    /// connected pin's strip, total free holes): higher is better.
    score: (bool, usize, usize),
}

const ROTATIONS: [Rotation; 4] = [
    Rotation::Deg0,
    Rotation::Deg90,
    Rotation::Deg180,
    Rotation::Deg270,
];

struct PartFits {
    straddle: Option<Fit>,
    left: Option<Fit>,
    right: Option<Fit>,
}

fn part_fits(
    definition: &ComponentDefinition,
    pitch_mm: f64,
    connected: &BTreeSet<String>,
) -> PartFits {
    let base_cover = footprint_cover(definition, pitch_mm);
    let base_access = access_cover(definition);
    let mut best = PartFits {
        straddle: None,
        left: None,
        right: None,
    };
    for rotation in ROTATIONS {
        let pins: Vec<(String, Point)> = definition
            .pins
            .iter()
            .map(|pin| {
                (
                    pin.id.clone(),
                    rotate(rotation, (i64::from(pin.x), i64::from(pin.y))),
                )
            })
            .collect();
        let cover: BTreeSet<Point> = base_cover.iter().map(|p| rotate(rotation, *p)).collect();
        let access: BTreeSet<Point> = base_access.iter().map(|p| rotate(rotation, *p)).collect();
        let top = cover.iter().map(|p| p.1).min().unwrap_or(0);
        let bottom = cover.iter().map(|p| p.1).max().unwrap_or(0);
        if bottom - top >= i64::from(BREADBOARD_ROWS) {
            continue;
        }
        let xs: BTreeSet<i64> = pins.iter().map(|(_, p)| p.0).collect();
        let make = |placement: PartPlacement, shift_x: i64| -> Option<Fit> {
            let mut fit = Fit {
                rotation,
                placement,
                shift_x,
                pins: pins.clone(),
                cover: cover.clone(),
                access: access.clone(),
                top,
                bottom,
                score: (false, 0, 0),
            };
            fit.score = fit_score(&fit, connected)?;
            Some(fit)
        };
        let consider = |slot: &mut Option<Fit>, candidate: Option<Fit>| {
            if let Some(candidate) = candidate
                && slot.as_ref().is_none_or(|old| candidate.score > old.score)
            {
                *slot = Some(candidate);
            }
        };
        // Straddle: exactly two pin columns, one each side of the channel.
        if xs.len() == 2 {
            let (first, last) = (*xs.first().unwrap_or(&0), *xs.last().unwrap_or(&0));
            let span = last - first;
            if (MIN_STRADDLE..=MAX_STRADDLE).contains(&span) {
                for left_column in LEFT_COLUMNS.map(i64::from) {
                    let right_column = left_column + span;
                    if RIGHT_COLUMNS.map(i64::from).contains(&right_column) {
                        consider(
                            &mut best.straddle,
                            make(PartPlacement::Straddle, left_column - first),
                        );
                    }
                }
            }
        }
        // One side: every pin on its own row, all within five columns. The
        // pins go to the columns nearest the channel, leaving the outer holes
        // for jumpers to the rails.
        let ys: BTreeSet<i64> = pins.iter().map(|(_, p)| p.1).collect();
        if !pins.is_empty() && ys.len() == pins.len() {
            let min_x = *xs.first().unwrap_or(&0);
            let max_x = *xs.last().unwrap_or(&0);
            if max_x - min_x <= 4 {
                consider(&mut best.left, make(PartPlacement::Left, 7 - max_x));
                consider(&mut best.right, make(PartPlacement::Right, 10 - min_x));
            }
        }
    }
    best
}

/// Scores a fit, or rejects it when a pin lands outside its side's strip, a
/// covered hole leaves the board's columns, or a connected pin's strip has no
/// free hole left for a jumper. Holes under the part's own access regions are
/// not free. A fit whose access regions face away from the channel, toward
/// the board edge, beats one facing it, whatever its free holes.
fn fit_score(fit: &Fit, connected: &BTreeSet<String>) -> Option<(bool, usize, usize)> {
    for &(x, _) in &fit.cover {
        let column = fit.shift_x + x;
        if !(0..i64::from(BOARD_COLUMNS)).contains(&column) {
            return None;
        }
    }
    let mut fewest = usize::MAX;
    let mut total = 0;
    for (pin_id, (x, y)) in &fit.pins {
        let column = u32::try_from(fit.shift_x + x).ok()?;
        let lane = Lane::of_column(column)?;
        match (fit.placement, lane) {
            (PartPlacement::Left, Lane::Right) | (PartPlacement::Right, Lane::Left) => return None,
            _ => {}
        }
        let free = lane
            .columns()
            .iter()
            .filter(|&&c| {
                let at = (i64::from(c) - fit.shift_x, *y);
                !fit.cover.contains(&at) && !fit.access.contains(&at)
            })
            .count();
        total += free;
        if connected.contains(pin_id) {
            if free == 0 {
                return None;
            }
            fewest = fewest.min(free);
        }
    }
    Some((faces_out(fit), fewest.min(LEFT_COLUMNS.len()), total))
}

/// Whether a fit's required access regions lie no nearer the channel than its
/// pins: a terminal on columns a–e takes its wires from the left, one on f–j
/// from the right. Regions above or below the pins (a straddling Nano's USB
/// end) count as facing out.
#[allow(clippy::cast_precision_loss)] // Footprints are a few dozen holes.
fn faces_out(fit: &Fit) -> bool {
    if fit.access.is_empty() || fit.pins.is_empty() {
        return true;
    }
    let channel = f64::from(LEFT_COLUMNS[4] + RIGHT_COLUMNS[0]) / 2.0;
    let mean = |xs: &mut dyn Iterator<Item = i64>| {
        let (sum, n) = xs.fold((0, 0), |(sum, n), x| (sum + x, n + 1));
        (sum + fit.shift_x * n) as f64 / n as f64
    };
    let pins = mean(&mut fit.pins.iter().map(|(_, p)| p.0));
    let access = mean(&mut fit.access.iter().map(|p| p.0));
    (access - channel).abs() + 0.25 >= (pins - channel).abs()
}

/// Whether a layout point lies within a board's outline, holes or not (the
/// channel and the gaps beside the rails included).
fn in_board_area(point: Point) -> bool {
    point.0 >= 0
        && point.1 >= 0
        && point.1 < i64::from(BREADBOARD_ROWS)
        && point.0 % i64::from(BOARD_STRIDE) < i64::from(BOARD_COLUMNS)
}

struct NetInfo {
    id: NetId,
    name: String,
    color: WireColor,
    /// Component pins, bridges excluded.
    pins: Vec<PinRef>,
}

/// Where a pin ended up.
#[derive(Clone, Copy)]
enum PinSpot {
    Board(Strip),
    Off(HolePos),
}

struct Planner<'a> {
    design: &'a Design,
    pitch_mm: f64,
    board_count: u32,
    /// Strips taken by a part (or a hub), so parts never share one.
    claimed: BTreeSet<Strip>,
    /// Cells of the board area under a part's body or required access region,
    /// holes or not: no two parts share one, so access regions never overlap
    /// each other or a body, over the channel included.
    area: BTreeSet<Point>,
    /// Holes with something in them, or under a part's body.
    used: BTreeSet<HolePos>,
    parts: Vec<BreadboardPart>,
    pins: BTreeMap<PinRef, PinSpot>,
    pin_labels: BTreeMap<HolePos, String>,
    off_board_x: i64,
    off_board_bottom: i64,
    rails: BTreeMap<RailSlot, usize>,
    used_rails: BTreeSet<(usize, u32, RailSlot)>,
    jumpers: Vec<(usize, JumperKind, HolePos, HolePos)>,
    warnings: Vec<BreadboardWarning>,
}

/// Lays the design out on breadboards (ADR 0018).
#[must_use]
pub fn breadboard_layout(design: &Design, revision: DocumentRevision) -> BreadboardLayout {
    let nets = net_infos(design);
    let mut planner = Planner {
        design,
        pitch_mm: design.board.pitch_mm,
        board_count: 1,
        claimed: BTreeSet::new(),
        area: BTreeSet::new(),
        used: BTreeSet::new(),
        parts: Vec::new(),
        pins: BTreeMap::new(),
        pin_labels: BTreeMap::new(),
        off_board_x: 0,
        off_board_bottom: 0,
        rails: BTreeMap::new(),
        used_rails: BTreeSet::new(),
        jumpers: Vec::new(),
        warnings: Vec::new(),
    };
    let proposed = design
        .nets
        .values()
        .filter(|net| net.state == ConnectionState::Proposed)
        .count();
    if proposed > 0 {
        planner.warn(
            "breadboard.proposed_nets",
            format!(
                "{proposed} proposed connection{} not laid out: accept or reject {} first.",
                if proposed == 1 { " is" } else { "s are" },
                if proposed == 1 { "it" } else { "them" },
            ),
            Vec::new(),
        );
    }
    planner.place_parts(&nets);
    planner.assign_rails(&nets);
    planner.connect(&nets);
    planner.finish(&nets, revision)
}

fn net_infos(design: &Design) -> Vec<NetInfo> {
    let is_bridge = |component_id: &ComponentInstanceId| {
        design
            .components
            .get(component_id)
            .and_then(|instance| design.component_definitions.get(&instance.definition_id))
            .is_none_or(|definition| definition.kind == ComponentKind::Bridge)
    };
    let mut nets: Vec<NetInfo> = design
        .nets
        .values()
        .filter(|net| net.state == ConnectionState::Accepted)
        .map(|net| NetInfo {
            id: net.id,
            name: net.name.clone(),
            color: WireColor::White,
            pins: net
                .pins
                .iter()
                .filter(|pin| !is_bridge(&pin.component_id))
                .cloned()
                .collect(),
        })
        .collect();
    nets.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    let mut palette = SIGNAL_COLORS.iter().cycle();
    for net in &mut nets {
        net.color = net_color(design, net.id, &net.name)
            .unwrap_or_else(|| palette.next().cloned().unwrap_or(WireColor::White));
    }
    nets
}

/// Colours for nets that have no wires to copy, in turn.
const SIGNAL_COLORS: [WireColor; 5] = [
    WireColor::Yellow,
    WireColor::Green,
    WireColor::Blue,
    WireColor::Orange,
    WireColor::White,
];

/// The colour most of a net's wires (or else rails) have on the perfboard; a
/// conventional colour for ground and supply nets without any.
fn net_color(design: &Design, net_id: NetId, name: &str) -> Option<WireColor> {
    fn count(counts: &mut Vec<(WireColor, usize)>, color: &WireColor) {
        if let Some(entry) = counts.iter_mut().find(|(c, _)| c == color) {
            entry.1 += 1;
        } else {
            counts.push((color.clone(), 1));
        }
    }
    let mut counts: Vec<(WireColor, usize)> = Vec::new();
    for wire in design.wires.values().filter(|wire| wire.net_id == net_id) {
        count(&mut counts, &wire.color);
    }
    if counts.is_empty() {
        for rail in design.rails.values().filter(|rail| rail.net_id == net_id) {
            count(&mut counts, &rail.color);
        }
    }
    // First most frequent, in first-seen order: wires are keyed by ID, so
    // this is deterministic.
    let mut best: Option<(WireColor, usize)> = None;
    for (color, n) in counts {
        if best.as_ref().is_none_or(|(_, m)| n > *m) {
            best = Some((color, n));
        }
    }
    best.map(|(color, _)| color).or_else(|| {
        if is_ground(name) {
            Some(WireColor::Black)
        } else if is_power(name) {
            Some(WireColor::Red)
        } else {
            None
        }
    })
}

fn is_ground(name: &str) -> bool {
    let upper = name.to_uppercase();
    ["GND", "VSS", "0V"].iter().any(|word| {
        upper == *word
            || upper.starts_with(&format!("{word}_"))
            || upper.ends_with(&format!("_{word}"))
    })
}

fn is_power(name: &str) -> bool {
    let upper = name.to_uppercase();
    upper.starts_with('+')
        || ["VCC", "VDD", "VIN", "VBUS", "5V", "3V3", "12V"]
            .iter()
            .any(|word| upper.contains(word))
}

impl<'a> Planner<'a> {
    fn warn(&mut self, code: &str, message: String, objects: Vec<ObjectReference>) {
        self.warnings.push(BreadboardWarning {
            code: code.to_owned(),
            message,
            objects,
        });
    }

    fn definition(&self, instance: &ComponentInstance) -> Option<&'a ComponentDefinition> {
        self.design
            .component_definitions
            .get(&instance.definition_id)
    }

    fn value(&self, instance: &ComponentInstance) -> Option<ValueReading> {
        let kind = self.definition(instance)?.value_kind;
        read_value(kind, &instance.value)
    }

    fn pin_label(&self, pin: &PinRef) -> String {
        let Some(instance) = self.design.components.get(&pin.component_id) else {
            return pin.pin_id.clone();
        };
        let name = self
            .definition(instance)
            .and_then(|definition| definition.pins.iter().find(|p| p.id == pin.pin_id))
            .map_or_else(
                || pin.pin_id.clone(),
                |p| {
                    if p.name.is_empty()
                        || p.name == p.number
                        || p.name.eq_ignore_ascii_case(&format!("pin {}", p.number))
                    {
                        format!("pin {}", p.number)
                    } else {
                        format!("{} {}", p.number, p.name)
                    }
                },
            );
        format!("{} · {name}", instance.name)
    }

    // ── Parts ────────────────────────────────────────────────────────────────

    fn place_parts(&mut self, nets: &[NetInfo]) {
        // Pins that need a jumper: those on a net with another pin.
        let mut connected: BTreeMap<ComponentInstanceId, BTreeSet<String>> = BTreeMap::new();
        for net in nets.iter().filter(|net| net.pins.len() >= 2) {
            for pin in &net.pins {
                connected
                    .entry(pin.component_id)
                    .or_default()
                    .insert(pin.pin_id.clone());
            }
        }
        let empty = BTreeSet::new();
        let mut instances: Vec<&ComponentInstance> = self
            .design
            .components
            .values()
            .filter(|instance| {
                self.definition(instance).is_some_and(|definition| {
                    definition.kind != ComponentKind::Bridge && !definition.pins.is_empty()
                })
            })
            .collect();
        instances.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));

        let mut on_board: Vec<(&ComponentInstance, PartFits)> = Vec::new();
        let mut off_board: Vec<&ComponentInstance> = Vec::new();
        for instance in instances {
            let Some(definition) = self.definition(instance) else {
                continue;
            };
            let fits = part_fits(
                definition,
                self.pitch_mm,
                connected.get(&instance.id).unwrap_or(&empty),
            );
            if fits.straddle.is_some() || fits.left.is_some() || fits.right.is_some() {
                on_board.push((instance, fits));
            } else {
                off_board.push(instance);
            }
        }

        // Biggest part first, then whichever shares most nets with the parts
        // already down, so connected parts end up near each other.
        let pin_nets: BTreeMap<&PinRef, NetId> = nets
            .iter()
            .flat_map(|net| net.pins.iter().map(move |pin| (pin, net.id)))
            .collect();
        let nets_of = |instance: &ComponentInstance| -> BTreeSet<NetId> {
            pin_nets
                .iter()
                .filter(|(pin, _)| pin.component_id == instance.id)
                .map(|(_, net)| *net)
                .collect()
        };
        let mut placed_nets: BTreeSet<NetId> = BTreeSet::new();
        while !on_board.is_empty() {
            let index = (0..on_board.len())
                .max_by_key(|&i| {
                    let (instance, _) = &on_board[i];
                    let shared = nets_of(instance).intersection(&placed_nets).count();
                    let pins = self.definition(instance).map_or(0, |d| d.pins.len());
                    // Earlier (by name) wins ties: max_by_key keeps the last
                    // maximum, so rank by reverse position.
                    (shared, pins, std::cmp::Reverse(i))
                })
                .unwrap_or(0);
            let (instance, fits) = on_board.remove(index);
            placed_nets.extend(nets_of(instance));
            self.place_on_board(instance, &fits);
        }

        self.off_board_x = i64::from(self.board_count * BOARD_STRIDE + OFF_BOARD_GAP);
        for instance in off_board {
            self.place_off_board(instance);
        }
    }

    fn place_on_board(&mut self, instance: &ComponentInstance, fits: &PartFits) {
        let candidates: Vec<&Fit> = if let Some(fit) = &fits.straddle {
            vec![fit]
        } else {
            [&fits.left, &fits.right].into_iter().flatten().collect()
        };
        loop {
            let mut best: Option<(u32, u32, &Fit)> = None;
            for fit in &candidates {
                if let Some((board, row)) = self.find_rows(fit)
                    && best.is_none_or(|(b, r, _)| (board, row) < (b, r))
                {
                    best = Some((board, row, fit));
                }
            }
            if let Some((board, row, fit)) = best {
                self.commit_part(instance, fit, board, row);
                return;
            }
            self.board_count += 1;
        }
    }

    /// Strips and holes a fit takes with its top cover row on `row`.
    fn fit_holes(fit: &Fit, board: u32, row: u32) -> Vec<HolePos> {
        let origin_x = i64::from(board * BOARD_STRIDE) + fit.shift_x;
        let origin_y = i64::from(row) - fit.top;
        fit.cover
            .iter()
            .map(|&(x, y)| hole((origin_x + x, origin_y + y)))
            .collect()
    }

    /// Board holes under a fit's required access regions with its top cover
    /// row on `row`. Beyond the boards is open air and needs no reserving.
    fn fit_access(fit: &Fit, board: u32, row: u32) -> Vec<HolePos> {
        let origin_x = i64::from(board * BOARD_STRIDE) + fit.shift_x;
        let origin_y = i64::from(row) - fit.top;
        fit.access
            .iter()
            .map(|&(x, y)| (origin_x + x, origin_y + y))
            .filter(|&(x, y)| x >= 0 && y >= 0)
            .map(hole)
            .filter(|h| strip_of(*h).is_some() || rail_of(*h).is_some())
            .collect()
    }

    /// Cells of the board area a fit's body and access regions take with its
    /// top cover row on `row`.
    fn fit_area(fit: &Fit, board: u32, row: u32) -> impl Iterator<Item = Point> + '_ {
        let origin_x = i64::from(board * BOARD_STRIDE) + fit.shift_x;
        let origin_y = i64::from(row) - fit.top;
        fit.cover
            .iter()
            .chain(&fit.access)
            .map(move |&(x, y)| (origin_x + x, origin_y + y))
            .filter(|p| in_board_area(*p))
    }

    /// The first board and top row where the fit's strips, and one row above
    /// and below them, are free, and its body and access regions overlap no
    /// other part's. Rows level with the rails come first.
    fn find_rows(&self, fit: &Fit) -> Option<(u32, u32)> {
        let height = u32::try_from(fit.bottom - fit.top).ok()?;
        let last = BREADBOARD_ROWS.saturating_sub(height);
        for board in 0..self.board_count {
            'rows: for row in (RAIL_FIRST_ROW.min(last)..last).chain(0..RAIL_FIRST_ROW.min(last)) {
                for covered in Self::fit_holes(fit, board, row) {
                    if self.used.contains(&covered) {
                        continue 'rows;
                    }
                    if let Some((b, lane, y)) = strip_of(covered) {
                        for near in [y.wrapping_sub(1), y, y + 1] {
                            if self.claimed.contains(&(b, lane, near)) {
                                continue 'rows;
                            }
                        }
                    }
                }
                if Self::fit_access(fit, board, row)
                    .iter()
                    .any(|h| self.used.contains(h))
                    || Self::fit_area(fit, board, row).any(|p| self.area.contains(&p))
                {
                    continue 'rows;
                }
                return Some((board, row));
            }
        }
        None
    }

    fn commit_part(&mut self, instance: &ComponentInstance, fit: &Fit, board: u32, row: u32) {
        for covered in Self::fit_holes(fit, board, row) {
            if let Some(strip) = strip_of(covered) {
                self.claimed.insert(strip);
            }
            self.used.insert(covered);
        }
        // Nothing goes where a cable plugs in: no jumper, hub or later part.
        self.used.extend(Self::fit_access(fit, board, row));
        self.area.extend(Self::fit_area(fit, board, row));
        let origin = (
            i64::from(board * BOARD_STRIDE) + fit.shift_x,
            i64::from(row) - fit.top,
        );
        for (pin_id, (x, y)) in &fit.pins {
            let at = hole((origin.0 + x, origin.1 + y));
            if let Some(strip) = strip_of(at) {
                self.pins.insert(
                    PinRef {
                        component_id: instance.id,
                        pin_id: pin_id.clone(),
                    },
                    PinSpot::Board(strip),
                );
            }
        }
        self.parts.push(BreadboardPart {
            component_id: instance.id,
            name: instance.name.clone(),
            value: self.value(instance),
            definition_id: instance.definition_id.clone(),
            position: hole(origin),
            rotation: fit.rotation,
            placement: fit.placement,
            board: Some(board),
        });
        if instance.shunted {
            self.warn(
                "breadboard.shunt",
                format!("Fit the shunt on {}, as on the perfboard.", instance.name),
                vec![component_ref(instance.id)],
            );
        }
    }

    /// Stacks a part that cannot sit on a board beside the boards, as it is
    /// turned on the perfboard.
    fn place_off_board(&mut self, instance: &ComponentInstance) {
        let Some(definition) = self.definition(instance) else {
            return;
        };
        let cover: Vec<Point> = footprint_cover(definition, self.pitch_mm)
            .into_iter()
            .map(|p| rotate(instance.rotation, p))
            .collect();
        let min_x = cover.iter().map(|p| p.0).min().unwrap_or(0);
        let min_y = cover.iter().map(|p| p.1).min().unwrap_or(0);
        let max_y = cover.iter().map(|p| p.1).max().unwrap_or(0);
        let origin = (self.off_board_x - min_x, self.off_board_bottom - min_y);
        self.off_board_bottom += max_y - min_y + 1 + OFF_BOARD_SPACING;
        for pin in &definition.pins {
            let (x, y) = rotate(instance.rotation, (i64::from(pin.x), i64::from(pin.y)));
            let at = hole((origin.0 + x, origin.1 + y));
            let pin_ref = PinRef {
                component_id: instance.id,
                pin_id: pin.id.clone(),
            };
            self.pin_labels.insert(at, self.pin_label(&pin_ref));
            self.pins.insert(pin_ref, PinSpot::Off(at));
        }
        self.parts.push(BreadboardPart {
            component_id: instance.id,
            name: instance.name.clone(),
            value: self.value(instance),
            definition_id: instance.definition_id.clone(),
            position: hole(origin),
            rotation: instance.rotation,
            placement: PartPlacement::OffBoard,
            board: None,
        });
        self.warn(
            "breadboard.off_board",
            format!(
                "{} cannot sit on a breadboard: its pins cannot each get a strip of their own. It is placed beside the boards; wire it in with male–female Dupont leads.",
                instance.name
            ),
            vec![component_ref(instance.id)],
        );
    }

    // ── Rails ────────────────────────────────────────────────────────────────

    /// Gives the four rails to the nets they simplify most. Ground and supply
    /// nets come first: with two of them, each gets a rail down both sides of
    /// the board. Rails they leave go to the nets with most pins (four or
    /// more). Fewer nets get a rail on both sides. The same rails carry the
    /// same nets on every board.
    fn assign_rails(&mut self, nets: &[NetInfo]) {
        let ranked = |rank: u8| {
            let mut list: Vec<(usize, usize)> = nets
                .iter()
                .enumerate()
                .filter(|(_, net)| net.pins.len() >= 2)
                .filter(|(_, net)| {
                    let kind = if is_ground(&net.name) {
                        0
                    } else if is_power(&net.name) {
                        1
                    } else {
                        2
                    };
                    kind == rank && (rank < 2 || net.pins.len() >= 4)
                })
                .map(|(index, net)| (usize::MAX - net.pins.len(), index))
                .collect();
            list.sort_unstable();
            list.into_iter().map(move |(_, index)| (rank, index))
        };
        let mut candidates: Vec<(u8, usize)> = ranked(0).chain(ranked(1)).collect();
        match candidates.len() {
            0 => candidates.extend(ranked(2)),
            1 => candidates.extend(ranked(2).take(2)),
            _ => {}
        }
        candidates.truncate(4);
        let wants_pair = |position: usize| match candidates.len() {
            0..=2 => true,
            3 => position == 0,
            _ => false,
        };
        let mut free: Vec<RailSlot> = RailSlot::ALL.to_vec();
        for (position, &(rank, index)) in candidates.iter().enumerate() {
            let preferred = if rank == 0 {
                RailPolarity::Minus
            } else {
                RailPolarity::Plus
            };
            let mut order: Vec<RailSlot> = free.clone();
            // Preferred polarity first, then left before right.
            order.sort_by_key(|slot| (slot.polarity() != preferred, slot.lane()));
            let Some(&first) = order.first() else { break };
            let mut taken = vec![first];
            if wants_pair(position)
                && let Some(&partner) = order
                    .iter()
                    .find(|slot| slot.polarity() == first.polarity() && slot.lane() != first.lane())
            {
                taken.push(partner);
            }
            for slot in taken {
                free.retain(|s| *s != slot);
                self.rails.insert(slot, index);
            }
        }
    }

    // ── Connections ──────────────────────────────────────────────────────────

    fn connect(&mut self, nets: &[NetInfo]) {
        // Rail nets first, so their strip-to-rail jumpers get the outer holes.
        let rail_nets: BTreeSet<usize> = self.rails.values().copied().collect();
        let order: Vec<usize> = rail_nets
            .iter()
            .copied()
            .chain((0..nets.len()).filter(|i| !rail_nets.contains(i)))
            .collect();
        for index in order {
            let net = &nets[index];
            let mut strips: Vec<Strip> = Vec::new();
            let mut off: Vec<HolePos> = Vec::new();
            for pin in &net.pins {
                match self.pins.get(pin) {
                    Some(PinSpot::Board(strip)) => {
                        if !strips.contains(strip) {
                            strips.push(*strip);
                        }
                    }
                    Some(PinSpot::Off(at)) => off.push(*at),
                    None => {}
                }
            }
            if strips.len() + off.len() < 2 {
                continue;
            }
            if rail_nets.contains(&index) {
                self.connect_to_rails(index, net, &strips, &off);
            } else {
                self.connect_strips(index, net, strips, &off);
            }
        }
    }

    fn net_slots(&self, index: usize) -> Vec<RailSlot> {
        self.rails
            .iter()
            .filter(|(_, net)| **net == index)
            .map(|(slot, _)| *slot)
            .collect()
    }

    fn connect_to_rails(&mut self, index: usize, net: &NetInfo, strips: &[Strip], off: &[HolePos]) {
        let slots = self.net_slots(index);
        for &strip in strips {
            let (board, lane, row) = strip;
            let slot = slots
                .iter()
                .copied()
                .find(|slot| slot.lane() == lane)
                .or_else(|| slots.first().copied());
            let Some(slot) = slot else { continue };
            let toward_edge = slot.lane() == lane;
            let from = self.take_strip_hole(strip, toward_edge);
            let to = self.take_rail_hole(board, slot, row);
            match (from, to) {
                (Some(from), Some(to)) => {
                    self.used_rails.insert((index, board, slot));
                    self.jumpers.push((index, JumperKind::Jumper, from, to));
                }
                _ => self.out_of_holes(net),
            }
        }
        // Leads from off-board pins go into the last board's rails, on the
        // side facing them.
        for &pin in off {
            let board = self.board_count - 1;
            let slot = slots
                .iter()
                .copied()
                .rev()
                .find(|slot| slot.lane() == Lane::Right)
                .or_else(|| slots.first().copied());
            let Some(slot) = slot else { continue };
            match self.take_rail_hole(board, slot, bb_coords(pin).1) {
                Some(to) => {
                    self.used_rails.insert((index, board, slot));
                    self.jumpers.push((index, JumperKind::Lead, pin, to));
                }
                None => self.out_of_holes(net),
            }
        }
        // Link the rails the net uses, left to right, just below the parts
        // they serve, where the link is seen with them (and, failing that, as
        // low on the board as there is room).
        let mut used: Vec<(u32, RailSlot)> = self
            .used_rails
            .iter()
            .filter(|(i, _, _)| *i == index)
            .map(|(_, board, slot)| (*board, *slot))
            .collect();
        used.sort_by_key(|(board, slot)| board * BOARD_STRIDE + slot.column());
        for pair in used.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let free = |row: u32| {
                !self.used.contains(&rail_hole(a.0, a.1, row))
                    && !self.used.contains(&rail_hole(b.0, b.1, row))
            };
            let below_parts = self
                .claimed
                .iter()
                .filter(|(board, _, _)| (a.0..=b.0).contains(board))
                .map(|(_, _, row)| row + 2)
                .max()
                .unwrap_or(RAIL_FIRST_ROW);
            // A row of its own, so links of different nets read apart.
            let clear = |row: u32| {
                [a.0, b.0].iter().all(|&board| {
                    RailSlot::ALL
                        .iter()
                        .all(|&slot| !self.used.contains(&rail_hole(board, slot, row)))
                })
            };
            let row = rail_rows()
                .find(|&row| row >= below_parts && clear(row))
                .or_else(|| rail_rows().find(|&row| row >= below_parts && free(row)))
                .or_else(|| rail_rows().rev().find(|&row| free(row)));
            if let Some(row) = row {
                let (from, to) = (rail_hole(a.0, a.1, row), rail_hole(b.0, b.1, row));
                self.used.insert(from);
                self.used.insert(to);
                self.jumpers.push((index, JumperKind::Jumper, from, to));
            } else {
                self.out_of_holes(net);
            }
        }
    }

    /// Joins a net's strips along a shortest spanning tree, never using more
    /// holes in a strip than it has free, then wires in off-board pins.
    fn connect_strips(
        &mut self,
        index: usize,
        net: &NetInfo,
        mut strips: Vec<Strip>,
        off: &[HolePos],
    ) {
        if !strips.is_empty() {
            let mut joined = vec![strips.remove(0)];
            while !strips.is_empty() {
                let mut best: Option<(i64, usize, usize)> = None;
                for (a, &from) in joined.iter().enumerate() {
                    if self.strip_free(from) == 0 {
                        continue;
                    }
                    for (b, &to) in strips.iter().enumerate() {
                        if self.strip_free(to) == 0 {
                            continue;
                        }
                        let d = distance(strip_center(from), strip_center(to));
                        if best.is_none_or(|(e, _, _)| d < e) {
                            best = Some((d, a, b));
                        }
                    }
                }
                let Some((_, a, b)) = best else {
                    self.out_of_holes(net);
                    break;
                };
                let (from, to) = (joined[a], strips.remove(b));
                if let (Some(h1), Some(h2)) = (
                    self.take_strip_hole(from, false),
                    self.take_strip_hole(to, false),
                ) {
                    self.jumpers.push((index, JumperKind::Jumper, h1, h2));
                }
                joined.push(to);
            }
            strips = joined;
        }
        for &pin in off {
            let target = point(pin);
            let nearest = strips
                .iter()
                .copied()
                .filter(|strip| self.strip_free(*strip) > 0)
                .min_by_key(|strip| (distance(strip_center(*strip), target), *strip));
            let strip = if let Some(strip) = nearest {
                strip
            } else {
                // No strip of this net to plug into: open a free one near the
                // pin and join it to the rest of the net.
                let hub = self.take_hub(target);
                if let Some(&other) = strips
                    .iter()
                    .filter(|strip| self.strip_free(**strip) > 0)
                    .min_by_key(|strip| distance(strip_center(**strip), strip_center(hub)))
                {
                    if let (Some(h1), Some(h2)) = (
                        self.take_strip_hole(hub, false),
                        self.take_strip_hole(other, false),
                    ) {
                        self.jumpers.push((index, JumperKind::Jumper, h1, h2));
                    }
                } else if !strips.is_empty() {
                    self.out_of_holes(net);
                }
                strips.push(hub);
                hub
            };
            // Leads come from the right: plug them in on the side facing them.
            match self.take_strip_hole(strip, strip.1 == Lane::Right) {
                Some(to) => self.jumpers.push((index, JumperKind::Lead, pin, to)),
                None => self.out_of_holes(net),
            }
        }
    }

    fn out_of_holes(&mut self, net: &NetInfo) {
        let code = "breadboard.out_of_holes";
        if self
            .warnings
            .iter()
            .any(|w| w.code == code && w.objects.iter().any(|o| o.id == net.id.to_string()))
        {
            return;
        }
        self.warn(
            code,
            format!(
                "{}: no free hole left for a jumper; finish this net by hand.",
                net.name
            ),
            vec![ObjectReference {
                kind: "net".to_owned(),
                id: net.id.to_string(),
            }],
        );
    }

    fn strip_free(&self, strip: Strip) -> usize {
        strip
            .1
            .columns()
            .iter()
            .filter(|&&column| !self.used.contains(&strip_hole(strip, column)))
            .count()
    }

    /// Takes a free hole in a strip: the one nearest the board edge, or the
    /// one nearest the channel.
    fn take_strip_hole(&mut self, strip: Strip, toward_edge: bool) -> Option<HolePos> {
        let mut columns = strip.1.outer_first();
        if !toward_edge {
            columns.reverse();
        }
        let found = columns
            .into_iter()
            .map(|column| strip_hole(strip, column))
            .find(|h| !self.used.contains(h))?;
        self.used.insert(found);
        Some(found)
    }

    /// Takes the free rail hole nearest a row.
    fn take_rail_hole(&mut self, board: u32, slot: RailSlot, row: u32) -> Option<HolePos> {
        let found = rail_rows()
            .map(|r| rail_hole(board, slot, r))
            .filter(|h| !self.used.contains(h))
            .min_by_key(|h| (bb_coords(*h).1.abs_diff(row), h.y))?;
        self.used.insert(found);
        Some(found)
    }

    /// Claims the free strip nearest a point, adding a board when there is
    /// none left.
    fn take_hub(&mut self, target: Point) -> Strip {
        loop {
            let mut best: Option<(i64, Strip)> = None;
            for board in 0..self.board_count {
                for lane in [Lane::Left, Lane::Right] {
                    for row in 0..BREADBOARD_ROWS {
                        let strip = (board, lane, row);
                        if self.claimed.contains(&strip) || self.strip_free(strip) < 5 {
                            continue;
                        }
                        let d = distance(strip_center(strip), target);
                        if best.is_none_or(|(e, s)| (d, strip) < (e, s)) {
                            best = Some((d, strip));
                        }
                    }
                }
            }
            if let Some((_, strip)) = best {
                self.claimed.insert(strip);
                return strip;
            }
            self.board_count += 1;
        }
    }

    // ── Output ───────────────────────────────────────────────────────────────

    #[allow(clippy::too_many_lines)] // One pass from plan to output keeps the numbering and labels in step.
    fn finish(mut self, nets: &[NetInfo], revision: DocumentRevision) -> BreadboardLayout {
        // A board added for a hub pushes the off-board parts right.
        let off_board_x = i64::from(self.board_count * BOARD_STRIDE + OFF_BOARD_GAP);
        let shift = off_board_x - self.off_board_x;
        if shift != 0 {
            let moved = |h: HolePos| {
                if i64::from(h.x) >= self.off_board_x {
                    hole((i64::from(h.x) + shift, i64::from(h.y)))
                } else {
                    h
                }
            };
            for part in self
                .parts
                .iter_mut()
                .filter(|p| p.placement == PartPlacement::OffBoard)
            {
                part.position = moved(part.position);
            }
            for jumper in &mut self.jumpers {
                jumper.2 = moved(jumper.2);
            }
            self.pin_labels = std::mem::take(&mut self.pin_labels)
                .into_iter()
                .map(|(h, label)| (moved(h), label))
                .collect();
        }

        let board_count = self.board_count;
        let multi = board_count > 1;
        let label = |h: HolePos| -> String {
            self.pin_labels
                .get(&h)
                .cloned()
                .unwrap_or_else(|| hole_label(h, multi))
        };
        let mut jumpers: Vec<BreadboardJumper> = self
            .jumpers
            .iter()
            .enumerate()
            .map(|(seq, &(index, kind, from, to))| {
                let net = &nets[index];
                (
                    (index, seq),
                    BreadboardJumper {
                        id: String::new(),
                        number: 0,
                        kind,
                        net_id: net.id,
                        net_name: net.name.clone(),
                        color: net.color.clone(),
                        from,
                        to,
                        from_label: label(from),
                        to_label: label(to),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>()
            .into_values()
            .collect();
        for (n, jumper) in jumpers.iter_mut().enumerate() {
            let number = u32::try_from(n + 1).unwrap_or(u32::MAX);
            jumper.number = number;
            jumper.id = format!("jumper:{number}");
        }

        let mut rails = Vec::new();
        for board in 0..board_count {
            for (slot, &index) in &self.rails {
                if self.used_rails.contains(&(index, board, *slot)) {
                    let net = &nets[index];
                    rails.push(BreadboardRail {
                        board,
                        slot: *slot,
                        net_id: net.id,
                        net_name: net.name.clone(),
                        color: net.color.clone(),
                    });
                }
            }
        }

        let has_off_board = self
            .parts
            .iter()
            .any(|p| p.placement == PartPlacement::OffBoard);
        let mut width = i64::from(board_count * BOARD_STRIDE - 1);
        let mut height = i64::from(BREADBOARD_ROWS);
        for part in self
            .parts
            .iter()
            .filter(|p| p.placement == PartPlacement::OffBoard)
        {
            let Some(instance) = self.design.components.get(&part.component_id) else {
                continue;
            };
            let Some(definition) = self.definition(instance) else {
                continue;
            };
            for (x, y) in footprint_cover(definition, self.pitch_mm) {
                let (rx, ry) = rotate(part.rotation, (x, y));
                width = width.max(i64::from(part.position.x) + rx + 1);
                height = height.max(i64::from(part.position.y) + ry + 1);
            }
        }
        let width = u32::try_from(width).unwrap_or(0);
        let height = u32::try_from(height).unwrap_or(0);
        let mut parts = self.parts;
        parts.sort_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then(a.component_id.cmp(&b.component_id))
        });
        let mut layout = BreadboardLayout {
            source_revision: revision,
            board_count,
            geometry: BreadboardGeometry {
                rows: BREADBOARD_ROWS,
                rail_first_row: RAIL_FIRST_ROW,
                rail_last_row: RAIL_LAST_ROW,
                rail_rows: rail_rows().collect(),
                board_columns: BOARD_COLUMNS,
                board_stride: BOARD_STRIDE,
                left_columns: LEFT_COLUMNS.to_vec(),
                right_columns: RIGHT_COLUMNS.to_vec(),
                rail_columns: RailSlot::ALL
                    .into_iter()
                    .map(|slot| RailColumn {
                        slot,
                        column: slot.column(),
                        polarity: slot.polarity(),
                    })
                    .collect(),
                off_board_x: if has_off_board {
                    u32::try_from(off_board_x).unwrap_or(width)
                } else {
                    width
                },
                width,
                height,
            },
            parts,
            rails,
            jumpers,
            warnings: self.warnings,
        };
        let problems = verify_breadboard_layout(self.design, &layout);
        for problem in problems {
            layout.warnings.push(BreadboardWarning {
                code: "breadboard.check_failed".to_owned(),
                message: problem,
                objects: Vec::new(),
            });
        }
        layout
    }
}

fn rail_hole(board: u32, slot: RailSlot, row: u32) -> HolePos {
    bb_hole(board * BOARD_STRIDE + slot.column(), row)
}

fn component_ref(id: ComponentInstanceId) -> ObjectReference {
    ObjectReference {
        kind: "component".to_owned(),
        id: id.to_string(),
    }
}

/// Breadboard notation for a hole: `e12`, `left − rail, row 7`; prefixed
/// with the board (`B2 e12`) when there are several.
fn hole_label(h: HolePos, multi: bool) -> String {
    let (x, y) = bb_coords(h);
    let board = x / BOARD_STRIDE;
    let column = x % BOARD_STRIDE;
    let prefix = if multi {
        format!("B{} ", board + 1)
    } else {
        String::new()
    };
    let row = y + 1;
    if let Some(slot) = RailSlot::from_column(column) {
        let side = match slot.lane() {
            Lane::Left => "left",
            Lane::Right => "right",
        };
        let sign = match slot.polarity() {
            RailPolarity::Minus => "−",
            RailPolarity::Plus => "+",
        };
        return format!("{prefix}{side} {sign} rail, row {row}");
    }
    let letters = "abcdefghij";
    let index = LEFT_COLUMNS
        .iter()
        .chain(RIGHT_COLUMNS.iter())
        .position(|c| *c == column);
    match index.and_then(|i| letters.get(i..=i)) {
        Some(letter) => format!("{prefix}{letter}{row}"),
        None if (8..=9).contains(&column) => format!("{prefix}channel, row {row}"),
        None => format!("{prefix}beside the rails, row {row}"),
    }
}

// ── Verification ─────────────────────────────────────────────────────────────

/// Union-find root of a point, compressing the path.
fn find(parent: &mut BTreeMap<HolePos, HolePos>, h: HolePos) -> HolePos {
    let mut root = h;
    while let Some(&next) = parent.get(&root) {
        if next == root {
            break;
        }
        root = next;
    }
    parent.insert(h, root);
    root
}

fn union(parent: &mut BTreeMap<HolePos, HolePos>, a: HolePos, b: HolePos) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent.insert(ra.max(rb), ra.min(rb));
    }
}

/// Rebuilds a layout's connectivity — strips, rails, jumpers, leads, pins —
/// and checks it against the design: every net joined, no two nets touching,
/// no hole used twice, nothing plugged into a covered hole. Returns one
/// message per problem; empty means the breadboard is the circuit.
#[must_use]
#[allow(clippy::too_many_lines)] // Every check side by side is easier to audit.
pub fn verify_breadboard_layout(design: &Design, layout: &BreadboardLayout) -> Vec<String> {
    let mut problems = Vec::new();
    let boards_end = layout.board_count * BOARD_STRIDE;
    let on_board =
        |h: HolePos| bb_coords(h).0 < boards_end && (strip_of(h).is_some() || rail_of(h).is_some());

    // Pin positions and covered holes of every laid-out part.
    let mut pin_at: BTreeMap<PinRef, HolePos> = BTreeMap::new();
    let mut occupied: BTreeMap<HolePos, String> = BTreeMap::new();
    let mut covered: BTreeSet<HolePos> = BTreeSet::new();
    // Holes under each part's required access regions, and every cell of the
    // board area a part's body or access region takes (channel included).
    let mut reserved: BTreeMap<HolePos, &str> = BTreeMap::new();
    let mut area: BTreeMap<Point, (&str, bool)> = BTreeMap::new();
    let mut overlaps: BTreeSet<(&str, &str)> = BTreeSet::new();
    for part in &layout.parts {
        let Some(definition) = design.component_definitions.get(&part.definition_id) else {
            problems.push(format!("{}: unknown definition", part.name));
            continue;
        };
        let origin = point(part.position);
        if part.placement != PartPlacement::OffBoard {
            let cells = footprint_cover(definition, design.board.pitch_mm)
                .into_iter()
                .map(|p| (p, false))
                .chain(access_cover(definition).into_iter().map(|p| (p, true)));
            for (p, is_access) in cells {
                let (x, y) = rotate(part.rotation, p);
                let at = (origin.0 + x, origin.1 + y);
                if !is_access {
                    covered.insert(hole(at));
                }
                if !in_board_area(at) {
                    continue;
                }
                match area.get(&at) {
                    Some(&(other, other_access))
                        if other != part.name.as_str() && overlaps.insert((other, &part.name)) =>
                    {
                        let place = hole_label(hole(at), true);
                        problems.push(match (other_access, is_access) {
                            (true, true) => format!(
                                "the access areas of {other} and {} overlap at {place}",
                                part.name
                            ),
                            (true, false) => {
                                format!("{} sits in {other}'s access area at {place}", part.name)
                            }
                            (false, true) => {
                                format!("{other} sits in {}'s access area at {place}", part.name)
                            }
                            (false, false) => {
                                format!("{other} and {} overlap at {place}", part.name)
                            }
                        });
                    }
                    Some(_) => {}
                    None => {
                        area.insert(at, (&part.name, is_access));
                    }
                }
            }
            for at in access_holes(definition, part.rotation, origin) {
                reserved.insert(at, &part.name);
            }
        }
        for pin in &definition.pins {
            let (x, y) = rotate(part.rotation, (i64::from(pin.x), i64::from(pin.y)));
            let at = hole((origin.0 + x, origin.1 + y));
            if part.placement != PartPlacement::OffBoard {
                if !on_board(at) || strip_of(at).is_none() {
                    problems.push(format!(
                        "{} pin {} is not in a strip",
                        part.name, pin.number
                    ));
                }
                if let Some(other) =
                    occupied.insert(at, format!("{} pin {}", part.name, pin.number))
                {
                    problems.push(format!(
                        "{other} and {} pin {} share a hole",
                        part.name, pin.number
                    ));
                }
            }
            pin_at.insert(
                PinRef {
                    component_id: part.component_id,
                    pin_id: pin.id.clone(),
                },
                at,
            );
        }
    }
    let off_pins: BTreeSet<HolePos> = layout
        .parts
        .iter()
        .filter(|p| p.placement == PartPlacement::OffBoard)
        .flat_map(|part| {
            pin_at
                .iter()
                .filter(move |(pin, _)| pin.component_id == part.component_id)
                .map(|(_, at)| *at)
        })
        .collect();

    // Union-find over points: holes and off-board pins.
    let mut parent: BTreeMap<HolePos, HolePos> = BTreeMap::new();
    for board in 0..layout.board_count {
        for row in 0..BREADBOARD_ROWS {
            for lane in [Lane::Left, Lane::Right] {
                let strip = (board, lane, row);
                let columns = lane.columns();
                for column in &columns[1..] {
                    union(
                        &mut parent,
                        strip_hole(strip, columns[0]),
                        strip_hole(strip, *column),
                    );
                }
            }
        }
        for slot in RailSlot::ALL {
            for row in rail_rows().skip(1) {
                union(
                    &mut parent,
                    rail_hole(board, slot, RAIL_FIRST_ROW),
                    rail_hole(board, slot, row),
                );
            }
        }
    }
    for jumper in &layout.jumpers {
        for end in [jumper.from, jumper.to] {
            let what = format!("jumper {}", jumper.number);
            if off_pins.contains(&end) {
                if jumper.kind != JumperKind::Lead {
                    problems.push(format!("{what} ends on an off-board pin but is not a lead"));
                }
            } else if !on_board(end) {
                problems.push(format!("{what} is not plugged into a hole at {end:?}"));
            } else if covered.contains(&end) {
                problems.push(format!(
                    "{what} is plugged in under a part at {}",
                    hole_label(end, true)
                ));
            }
            if let Some(other) = occupied.insert(end, what.clone()) {
                problems.push(format!(
                    "{other} and {what} share {}",
                    hole_label(end, true)
                ));
            }
        }
        union(&mut parent, jumper.from, jumper.to);
    }

    // Required access regions stay clear of jumpers.
    for jumper in &layout.jumpers {
        for end in [jumper.from, jumper.to] {
            if let Some(name) = reserved.get(&end) {
                problems.push(format!(
                    "jumper {} is plugged into {name}'s access area at {}",
                    jumper.number,
                    hole_label(end, true)
                ));
            }
        }
    }

    // Every net joined; no two nets, or a net and a free pin, touching.
    let nets = net_infos(design);
    let mut owner: BTreeMap<HolePos, &str> = BTreeMap::new();
    let mut netted: BTreeSet<&PinRef> = BTreeSet::new();
    for net in &nets {
        let roots: BTreeSet<HolePos> = net
            .pins
            .iter()
            .filter_map(|pin| pin_at.get(pin))
            .map(|at| find(&mut parent, *at))
            .collect();
        if roots.len() > 1 {
            problems.push(format!("{} is split into {} groups", net.name, roots.len()));
        }
        for root in roots {
            if let Some(other) = owner.insert(root, &net.name) {
                problems.push(format!("{other} and {} are shorted", net.name));
            }
        }
        netted.extend(net.pins.iter());
    }
    for (pin, at) in &pin_at {
        if netted.contains(pin) {
            continue;
        }
        let root = find(&mut parent, *at);
        if let Some(net) = owner.get(&root) {
            let name = design
                .components
                .get(&pin.component_id)
                .map_or("?", |c| c.name.as_str());
            problems.push(format!("{name} pin {} is shorted to {net}", pin.pin_id));
        }
    }
    problems
}
