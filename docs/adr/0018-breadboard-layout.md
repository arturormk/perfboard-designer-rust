# ADR 0018: Breadboard layouts are derived, readable over dense, and self-checked

- Status: accepted
- Date: 2026-11-15

## Context

Before soldering a perfboard, a maker often wants to try the circuit on a
solderless breadboard (a "protoboard"). The design already knows everything
needed: the parts and their footprints, the nets, and the colour of every
wire. Doing the translation by hand is slow and error-prone.

The target is the standard full-size board: 63 rows; per row two strips of
five connected holes (a–e, f–j) either side of a central channel two holes
wide; down each side a − and a + rail, connected from row 3 to row 61, with
its holes in groups of five (rows 3–7, 9–13, … 57–61) and one row left out
between groups. More
boards are joined side by side when one is not tall enough.

Several things pull against each other:

- A dense layout uses fewer boards, but crowded strips are hard to read and
  easy to get wrong on the bench. The user asked for jumpers over cramming.
- Not every part fits. A 2×N header, or a module whose pin rows are too far
  apart to straddle the channel, would short pins that share a strip.
- The four rails can each carry any net, but ground and supply are where they
  help most.
- The view is for checking a circuit once, not for production, so it must
  not need editing, and must never drift from the design.

## Decision

1. **The layout is a pure function of the design.**
   `perfboard_project::breadboard_layout(&Design, DocumentRevision)` returns a
   `BreadboardLayout`: boards, part placements, rail nets, numbered jumpers,
   and warnings. It is computed on request (Tauri `get_breadboard_layout`,
   MCP `get_breadboard_layout`) and never stored. The same design always
   yields the same layout. The UI shows it read-only, with pan and zoom.

2. **Coordinates** are one unit per hole, rows down, boards left to right at
   a stride of 19 columns. Within a board: rails at columns 0, 1, 16 and 17
   (each pair + then − from left to right: red +, blue − … red +, blue −);
   strips at 3–7 (a–e) and 10–14 (f–j). The layout sends
   these dimensions with every result, including the rows that have rail
   holes, so a viewer draws exactly what was assumed.

3. **Part placement.** Each part is tried in all four rotations:
   - _Straddle_ when its pins form exactly two columns 3–11 holes apart: one
     column each side of the channel (DIPs, the Nano, axial resistors across
     the gap). The offset that leaves the most free holes per strip wins.
   - _One side_ when every pin can take its own row within five columns, pins
     at the columns nearest the channel so outer holes stay free for rail
     jumpers.
   - _Off the board_ otherwise: placed beside the last board, as turned on the
     perfboard, and wired in with male–female Dupont leads.

   Holes a part covers are its pins, its declared occupied holes, and every
   hole strictly under its body outline. A placement is rejected when a pin
   that needs a jumper would have no free hole in its strip.

   A part's _required access regions_ (where a cable plugs in or a screw is
   turned, such as a Nano's USB end) are kept clear: a placement is only
   taken where those holes are free, and they are then reserved, so no
   jumper, hub or later part goes there. Holes under the part's own access
   regions do not count as free for its pins, so the offset chosen still
   leaves jumper holes beside them. Parts of a region beyond the boards lie
   over open air and need nothing. Optional regions are ignored.

   Access regions face away from the channel: among fits that work, one
   whose regions lie no nearer the channel than its pins always wins, so a
   terminal on columns a–e takes its wires from the left and one on f–j from
   the right. No two parts' bodies or access regions share a cell of the
   board area, holes or not, so regions cannot meet over the channel.

   Parts go down biggest first, then whichever shares most nets with those
   already placed, at the first board and row where all their strips, and
   one row above and below, are free. Parts never share a strip. A new board
   is added when nothing fits.

   On that board, the fit with fewest pins away from their net's rails wins
   before the first row does: a straddling part is turned end for end, or a
   one-side part goes on the other side, so a diode between two supplies
   takes a short jumper to each rail instead of two across the board. Rails
   are assigned from the nets before any part goes down so this is known.
   A part with required access regions keeps its one best turn, so a Nano's
   USB end stays at the top of the board.

4. **Rails.** Ground and supply nets (by name) come first. With two of them,
   each gets a rail down both sides; ground takes the − rails. Rails they
   leave go to nets with four or more pins. The same rails carry the same
   nets on every board, and the rails a net actually uses are linked with a
   jumper on the first free rail row below the parts they serve (each link
   on a row of its own), so the link that makes, say, ground on the left the
   same as ground on the right is seen with the circuit rather than dozens of
   rows away. The panel lists these links by net.

5. **Jumpers.** A rail net's strips each get a short jumper to the nearest
   rail of the net, preferably on the same side. Other nets are joined strip
   to strip along a shortest spanning tree that never uses more holes in a
   strip than it has free. An off-board pin's lead goes straight to its net's
   rail, or to the nearest strip of its net; when the net has none, a free
   strip is opened as a hub. Every jumper takes the colour most of the net's
   perfboard wires have, otherwise black for ground, red for supply, and a
   rotating palette for other nets.

6. **Self-check.** `verify_breadboard_layout` rebuilds the copper
   connectivity from strips, rails, jumpers and pins, and checks it against
   the design: every net joined, no two nets touching, no free pin shorted,
   no hole used twice, nothing plugged in under a part, no two parts'
   bodies or access regions overlapping (over the channel included), and no
   jumper in a required access region. The layout runs it on
   itself and reports any mismatch as a `breadboard.check_failed` warning, so
   a layout bug shows up as a visible note, never as a silent short.

7. **What is left out.** Only accepted nets are laid out; proposed ones get a
   warning. Bridges are perfboard copper with no breadboard part, so their
   pins are ignored and the nets they serve are joined with jumpers. A part
   lead on no net that copper still joins to one (in the same hole as a
   bridge end or another pin of the net, at the far end of a run of bridges,
   or through a rail) is laid out as a pin of that net; otherwise it would be
   left unwired on the breadboard while soldered in on the perfboard. Jumper
   headers are placed like any two-pin part; a fitted shunt gets a reminder.

## Consequences

- The breadboard always matches the current design, including MCP edits, and
  the self-check makes "the breadboard is the circuit" a tested property
  rather than a hope.
- Layouts are deliberately roomy and may use more boards than an expert
  would. That is the intended trade.
- Rail stripes follow one convention (+ then − on each side, left to right);
  physical boards vary, so the view labels each used rail with its net.
- Keeping access regions clear costs rows, and can push a part with a
  region beside its pins off the board when no offset leaves a jumper hole.
  That is preferred to a layout where the cable cannot be plugged in.
- A module that could fit by spanning two boards is placed off the board
  instead. Spanning boards is fragile on the bench and is not attempted.
- Heuristics are local (greedy placement, spanning tree per net), so the
  layout can be improved later without changing its contract.
