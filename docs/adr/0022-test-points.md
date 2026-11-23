# ADR 0022: Test points predict only what the design proves

- Status: accepted (decisions 1–5 and 7 implemented; 6 to follow)
- Date: 2026-11-23

## Context

A built board, perfboard or breadboard, should be checked with a multimeter
before and after power goes on. The assembly plan (ADR 0012) already ends
with check steps: a visual inspection, continuity per accepted net, and a
supply-to-ground resistance check when those nets can be recognised by name.
Each measurement is a `Probe` carrying its text, its nets and the holes to
touch. The breadboard layout (ADR 0018) has no checks at all.

Two things would make the checks far more useful:

- **Expected readings.** "Measure across R3" helps little; "expect 4.7 kΩ
  ±1%" catches a misread colour code or a lead in the wrong row at once.
- **Breadboard checks.** On a breadboard the strips are made in the factory,
  so a net's continuity between two holes of one strip proves nothing. The
  mistakes there are different: a lead one row off, a part not pushed home,
  a bent lead touching the next strip, a rail split at mid-board.

This edges into circuit analysis. The design knows the nets, which parts are
resistors and capacitors (from their `ValueKind`), their values and colour-
code tolerance (`BandCount`: 1% or 5%), bridges, and fitted jumper shunts. It
does not know what any other part does: diodes, transistors and ICs may
conduct under a meter's test voltage, depending on polarity, and pins carry
no electrical type. Supply voltages are not stored; supply and ground are
guessed from net names.

A check that states a wrong expected value is worse than no check: the
builder hunts for a fault that is not there, or stops trusting the list.

## Decision

1. **One derived model, two views.** `perfboard_project::test_points`
   computes the expected readings from the design alone: pure functions,
   never stored, like the assembly plan and the breadboard layout. A reading
   names nets and the pins to touch. Each view maps those pins to its own
   holes: the assembly plan's check steps to pin holes on the perfboard, the
   breadboard layout's new `checks` to the part's lead (see 4). The expected
   values and their wording are therefore the same in both views.

2. **Only predict what is provable.** Every expected value is either exact
   under the model below or left out. A measurement that cannot be predicted
   is not shown with a guess; the step says how many were skipped and why
   ("R1 (10 kΩ): not predicted, D1 also joins its nets."). Being silent is
   allowed; being wrong is not.

3. **Measurements carry structure as well as text.** `Probe`, and the
   breadboard's `BreadboardProbe`, carry a `measure` field:
   - `continuity`: the points should beep;
   - `open`: no connection is expected;
   - `resistance { nominal, min, max }` in ohms;
   - `voltage { volts }` against ground, power on (with decision 6).

   The text stays the readable form. The structure lets the viewer show the
   expected reading and lets an MCP agent reason about a reported one.

4. **Breadboard checks probe leads, not strips.** For the breadboard the
   plan adds:
   - _Continuity at the leads_: each net's pins measured on the part's own
     leads or pins, which catches a part in the wrong row and one not seated.
   - _Isolation between neighbours_: `open` between strips on adjacent rows
     on the same side that carry different nets, and between the + and −
     rails on each side. This catches bent leads and stray jumper ends.
   - _Rails end to end_: `continuity` between the first and last jumper on
     every used rail, because many boards split their rails at mid-board
     where ADR 0018 assumes none. Rail link jumpers need no check of their
     own: a net's continuity at the leads already crosses them.

   Perfboard checks keep their pin-hole continuity and inspection steps.

5. **Resistance model (power off).** Nets are nodes; parts become elements:
   - a `Resistance` part with exactly two pins and a value is a resistor,
     with the tolerance of its colour code; a zero-ohm link, a bridge and a
     jumper with a fitted shunt merge their nets;
   - a capacitor, and a jumper without its shunt, is open (the step notes
     that a capacitor's reading climbs while it charges);
   - any other part with pins on two or more nets, and a resistor with no
     value or other than two pins (potentiometers, networks), is an unknown
     element joining all of those nets.

   The reading between nets A and B is predicted only if no unknown element
   lies in a biconnected block on the path from A to B in the block–cut
   tree. Elements outside those blocks cannot change the reading, so the
   effective resistance is then exact: the blocks are solved in series, each
   with a small nodal (Laplacian) solve. Effective resistance only grows when
   a resistor grows, so `min` and `max` are the readings with every resistor
   at the low and the high end of its tolerance; the meter's own accuracy is
   left to the builder and said so in the step text.

   The plan predicts:
   - each resistor's in-circuit reading across its own nets, nominal when
     nothing parallels it, otherwise with the parallel path named;
   - supply to ground, replacing today's "not near 0 Ω" when the value is
     provable, and keeping it when not;
   - `open` between nets joined by no element at all.

6. **Voltage model (power on), deliberately narrow.** A net gets a supply
   voltage from a new optional net field, `voltage`, or failing that from its
   name (`+5V`, `5V`, `3V3`, `12V`, `-12V`; `VCC` and `VDD` give none).
   Ground is 0 V. For every net with a known voltage, every component pin on
   it gets a `voltage` measurement against ground: "+5V: expect 5 V at
   U1 · 8, U2 · 14". No other node is predicted. Divider midpoints and bias
   points depend on what loads them, which pin types would be needed to
   know; that is left for a later ADR.

7. **Order of steps.** Power off first: inspection, continuity, isolation
   and rails (breadboard), resistances, shorts. Then power on: voltages,
   which therefore never come first. IDs stay `check:<name>` so completion
   marks survive (ADR 0012): `check:resistance` is new in the assembly plan;
   the breadboard has `check:leads`, `check:neighbours`, `check:rails`,
   `check:resistance` and `check:power`.

## Consequences

- Breadboard and perfboard builds get the same expected values, computed
  once, and the breadboard finally has checks of its own.
- Circuits that are mostly resistors and capacitors get many predictions;
  circuits built around ICs and semiconductors get few resistance
  predictions, often only for resistors to a supply with nothing else in
  their block. That is the price of never being wrong, and the step shows
  what it skipped.
- Supply voltages become design data. Named supplies work without it; an
  ambiguous name needs the new field, set in the net panel or by MCP.
- `Probe` grows a field; old clients that ignore it still show the text.
- Transistor bias, LED current, op-amp outputs and anything else that needs
  a simulation are out of scope. Exporting a SPICE netlist would be the way
  to reach them, not growing this model.
