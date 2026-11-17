# ADR 0019: Part values are typed by the definition, normalised once, and read in Rust

- Status: accepted
- Date: 2026-11-17

## Context

A resistor or capacitor is useless on the bench without its value, and so
far the only place to write it was the part's free-form notes. The Board,
Assembly and Breadboard views could not show it, and the assembly plan could
not say which part to pick up. On the two physical views a resistor's value
is most useful as what the builder actually sees: its colour bands.

Values come in two sorts. Resistances and capacitances have a meaning that
can be checked (`4k7` and `4.7k` are the same part; `2m2` is almost certainly
a typo) and a code to show (bands, `104`). Everything else, such as `1N4148`
or `red`, is free text.

ADR 0004 forbids duplicating domain rules in TypeScript, and the MCP server
must see the same values and codes as the UI.

## Decision

1. **A first-class field.** `ComponentInstance` has an optional `value`
   string, not a key in the free-form `properties` map, because renderers,
   the assembly planner, the breadboard layout and MCP all read it. It is
   skipped when empty, so the project format stays at version 2.

2. **The definition says what a value means.** `ComponentDefinition` has an
   optional `value_kind`: `text` (default), `resistance` or `capacitance`.
   Nothing is inferred from the designator or the definition ID, so user parts
   opt in by declaring it. It counts as metadata, so placing a newer library
   part refreshes a project's stored copy, as "Update metadata" does.

3. **Normalised on entry, stored in display form.** The new operation
   `set_component_value` (and `add_component`) checks the value against the
   kind and stores it in one display form: engineering notation with a prefix
   and unit, `4.7 kΩ`, `470 mΩ`, `100 nF`. Invalid input is an operation
   error naming the part and showing how to write it. Storing the display form
   means every view, the file and MCP show the same text without formatting
   code anywhere else. A dedicated operation, rather than a field added to
   `change_instance_properties`, keeps older callers of that operation from
   clearing values they do not know about.

4. **Read tolerantly.** A stored value that does not read as its kind (written
   before the definition declared one) is shown as it is, with no code. Views
   never fail on a value.

5. **Codes are derived in Rust.** `read_value` returns the display text, the
   resistor bands and the capacitor code. Assembly steps, the parts list and
   breadboard parts carry that reading; the frontend only paints it. Bands
   are always five at 1% tolerance (three digits, multiplier, brown), one
   black band for a zero-ohm link, and none for values below 1 Ω or needing
   four significant digits. The tolerance is one constant, so a per-part
   tolerance can be added later.

6. **Readable direction.** The tolerance band is drawn after a wider gap and
   at the right or bottom end as seen on screen, whatever the part's
   rotation, so the code always reads left to right or top to bottom. Bands
   are drawn only on bodies long enough to be a lying resistor; a standing
   one shows its value as text.

7. **Where values show.** Board: under the part name, off by default, with a
   View toggle; the hover card always shows it. Assembly and Breadboard:
   always, with bands on resistor bodies on the component side. The assembly
   plan adds the value to the step title, a "Check the colour bands: …" line,
   a `parts` list grouped by type and value (smallest first), and an
   `assembly.value_missing` note for resistors and capacitors without one.
   Step order does not change.

## Consequences

- Changing a value changes the component step's fingerprint, so its
  checkmark is cleared: the part on the board is now the wrong one.
- Existing projects keep showing values as text until their part
  definitions are refreshed from the library.
- Values written in notes are not migrated.
- A bare three-digit capacitance is always a code (`100` is 10 pF); the
  normalised display shows the reading at once, so a misreading is easy to
  spot and fix.
