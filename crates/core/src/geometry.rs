use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct HolePos {
    pub x: u32,
    pub y: u32,
}

impl HolePos {
    #[must_use]
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn as_route_pos(self) -> RoutePos {
        RoutePos::new(i64::from(self.x) * 2, i64::from(self.y) * 2)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoutePos {
    pub x2: i64,
    pub y2: i64,
}

impl RoutePos {
    #[must_use]
    pub const fn new(x2: i64, y2: i64) -> Self {
        Self { x2, y2 }
    }

    #[must_use]
    pub const fn is_hole(self) -> bool {
        self.x2 % 2 == 0 && self.y2 % 2 == 0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rotation {
    Deg0,
    Deg90,
    Deg180,
    Deg270,
}

impl Rotation {
    #[must_use]
    pub const fn rotate_local(self, point: (i32, i32)) -> (i32, i32) {
        let (x, y) = point;
        match self {
            Self::Deg0 => (x, y),
            Self::Deg90 => (-y, x),
            Self::Deg180 => (-x, -y),
            Self::Deg270 => (y, -x),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Component,
    Solder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct BoardSize {
    holes_x: u32,
    holes_y: u32,
}

#[derive(Deserialize)]
struct BoardSizeFields {
    holes_x: u32,
    holes_y: u32,
}

impl<'de> Deserialize<'de> for BoardSize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let fields = BoardSizeFields::deserialize(deserializer)?;
        Self::new(fields.holes_x, fields.holes_y).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum GeometryError {
    #[error("a board must have at least one hole on each axis")]
    EmptyBoard,
    #[error("position ({x}, {y}) is outside the {holes_x} x {holes_y} board")]
    HoleOutsideBoard {
        x: u32,
        y: u32,
        holes_x: u32,
        holes_y: u32,
    },
    #[error("route position ({x2}, {y2}) is outside the board")]
    RouteOutsideBoard { x2: i64, y2: i64 },
}

impl BoardSize {
    /// Creates rectangular board extents from hole counts.
    ///
    /// # Errors
    ///
    /// Returns [`GeometryError::EmptyBoard`] when either axis has no holes.
    pub fn new(holes_x: u32, holes_y: u32) -> Result<Self, GeometryError> {
        if holes_x == 0 || holes_y == 0 {
            return Err(GeometryError::EmptyBoard);
        }
        Ok(Self { holes_x, holes_y })
    }

    #[must_use]
    pub const fn holes_x(self) -> u32 {
        self.holes_x
    }

    #[must_use]
    pub const fn holes_y(self) -> u32 {
        self.holes_y
    }

    #[must_use]
    pub const fn intervals_x(self) -> u32 {
        self.holes_x - 1
    }

    #[must_use]
    pub const fn intervals_y(self) -> u32 {
        self.holes_y - 1
    }

    /// Mirrors a canonical hole for solder-side presentation.
    ///
    /// # Errors
    ///
    /// Returns [`GeometryError::HoleOutsideBoard`] for a position outside the board.
    pub fn mirror_hole(self, position: HolePos) -> Result<HolePos, GeometryError> {
        if position.x >= self.holes_x || position.y >= self.holes_y {
            return Err(GeometryError::HoleOutsideBoard {
                x: position.x,
                y: position.y,
                holes_x: self.holes_x,
                holes_y: self.holes_y,
            });
        }
        Ok(HolePos::new(self.intervals_x() - position.x, position.y))
    }

    /// Mirrors a canonical half-grid route position for solder-side presentation.
    ///
    /// # Errors
    ///
    /// Returns [`GeometryError::RouteOutsideBoard`] for a position outside the board.
    pub fn mirror_route(self, position: RoutePos) -> Result<RoutePos, GeometryError> {
        let horizontal_limit = i64::from(self.intervals_x()) * 2;
        let vertical_limit = i64::from(self.intervals_y()) * 2;
        if !(0..=horizontal_limit).contains(&position.x2)
            || !(0..=vertical_limit).contains(&position.y2)
        {
            return Err(GeometryError::RouteOutsideBoard {
                x2: position.x2,
                y2: position.y2,
            });
        }
        Ok(RoutePos::new(horizontal_limit - position.x2, position.y2))
    }

    #[must_use]
    pub fn grid_span_mm(self, pitch_mm: f64) -> (f64, f64) {
        (
            f64::from(self.intervals_x()) * pitch_mm,
            f64::from(self.intervals_y()) * pitch_mm,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_count_intervals_not_holes() {
        let board = BoardSize::new(25, 18).expect("valid board");
        assert_eq!(board.grid_span_mm(2.54), (60.96, 43.18));
    }

    #[test]
    fn component_view_uses_canonical_coordinates() {
        let position = HolePos::new(3, 7);
        assert_eq!(position, HolePos::new(3, 7));
    }

    #[test]
    fn solder_view_mirrors_rectangular_board_edge_holes() {
        let board = BoardSize::new(25, 18).expect("valid board");
        assert_eq!(
            board.mirror_hole(HolePos::new(0, 0)),
            Ok(HolePos::new(24, 0))
        );
        assert_eq!(
            board.mirror_hole(HolePos::new(24, 17)),
            Ok(HolePos::new(0, 17))
        );
        assert_eq!(
            board.mirror_hole(HolePos::new(4, 10)),
            Ok(HolePos::new(20, 10))
        );
    }

    #[test]
    fn solder_view_mirrors_half_grid_routes() {
        let board = BoardSize::new(4, 3).expect("valid board");
        assert_eq!(
            board.mirror_route(RoutePos::new(1, 3)),
            Ok(RoutePos::new(5, 3))
        );
    }

    #[test]
    fn mirroring_twice_returns_the_original_position() {
        let board = BoardSize::new(9, 4).expect("valid board");
        let position = RoutePos::new(7, 2);
        assert_eq!(
            board.mirror_route(board.mirror_route(position).unwrap()),
            Ok(position)
        );
    }

    #[test]
    fn rotations_are_integer_and_clockwise() {
        assert_eq!(Rotation::Deg90.rotate_local((2, 1)), (-1, 2));
        assert_eq!(Rotation::Deg270.rotate_local((2, 1)), (1, -2));
    }
}
