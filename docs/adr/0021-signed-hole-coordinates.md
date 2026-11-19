# ADR 0021: Signed hole coordinates

- Status: accepted
- Date: 2026-11-19

## Context

ADR 0003 made hole coordinates (`HolePos`) unsigned. A part could therefore
sit off the board to the right or below, where its position stays positive,
but never to the left or above. Placing a new part often means parking it
beside the board to rotate it first, and later we may let parts live off the
board for good (connectors, panel parts) and wire to them. Both need the space
on every side of the board.

## Decision

`HolePos` holds signed 32-bit coordinates. Hole (0, 0) is still the board's
top-left hole; negative values lie left of or above the board. Whether a
position is a board hole is asked of the board (`BoardSize::contains_hole`),
not inferred from the type.

Only component positions use the extra range. Board features, saddles,
junctions and constraint areas must still lie on the board, and are checked as
before. A part with pins off the board is a placement problem
(`placement.out_of_bounds`), which the desktop app allows as a temporary state.

## Consequences

Existing files read unchanged: every stored coordinate was non-negative, and
JSON numbers carry no sign type. No schema version change is needed. Files
written now may hold negative component positions, which older builds reject.
Breadboard layouts keep their own non-negative grid and convert at the edges.
