# Component values development plan

Status: accepted
Scope: part values (resistance, capacitance, free text) shown on the Board,
Assembly and Breadboard tabs, with resistor colour codes on the two physical
views

## 1. Outcome

A user can give any placed part a value, such as `10k`, `100n`, `1N4148` or
`red`. Resistors and capacitors understand theirs: what is typed is checked
and normalised (`4k7` → `4.7 kΩ`, `0.1u` → `100 nF`), so a typo is visible at
once. Values appear:

- **Board:** as a small second line under the part name. This is off by
  default and can be turned on and off with a view toggle.
- **Assembly:** in the step title and instructions, in the step panel and on
  the drawing. Resistors also show their colour bands. A new parts list groups
  the parts by value.
- **Breadboard:** on the part. Resistors also show their colour bands.
- **MCP:** in `get_design`, `get_assembly_plan` and `get_breadboard_layout`.
  `apply_operations` can set values.

Resistor colour codes always use **five bands at 1% tolerance**: three digits,
a multiplier and a brown tolerance band. One constant holds the tolerance,
so a per-part tolerance can be added later without reworking anything.

Today people write values into the part notes (the Inspector placeholder
says "Assembly notes, values, reminders…"). Those notes stay as they are;
nothing is migrated automatically.

## 2. Design decisions

### 2.1 The value is a field, not a property

`ComponentInstance` gets a dedicated field:

```rust
/// The part's value as printed or coded on it: `10k`, `100n`, `1N4148`.
/// Resistances and capacitances are stored in canonical form.
#[serde(default, skip_serializing_if = "String::is_empty")]
pub value: String,
```

The free-form `properties` map is left alone. A value is read by the renderer,
the assembly planner, the breadboard layout and the MCP schema, so it should be
typed and documented rather than a magic key. The field is optional and
skipped when empty, so existing projects load unchanged and the project format
stays at v2. This is an additive change, recorded in
`docs/format/project-v2.md`.

### 2.2 The definition says what the value means

`ComponentDefinition` gets:

```rust
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    #[default]
    Text,
    Resistance,
    Capacitance,
}

#[serde(default, skip_serializing_if = "is_text_value")]
pub value_kind: ValueKind,
```

Every part can carry a value. `Text` is shown as typed. `Resistance` and
`Capacitance` are parsed, normalised and rejected if invalid. Nothing is
inferred from the designator or the definition id, so user-made parts can opt
in by setting `value_kind` in their TOML.

The built-in definitions get a minor version bump with the new field:

| Definition | `value_kind` |
| --- | --- |
| `resistor_3_holes`, `resistor_4_holes`, `resistor-standing` | `resistance` |
| `capacitor-ceramic-*`, `capacitor-electrolytic-*` | `capacitance` |

Existing projects keep their snapshot of each definition (ADR 0014). Until
the user updates a part's definition through the existing version-upgrade
path, its value is treated as text: shown, but not normalised and with no
bands. This is deliberate. Guessing from `designator = "R"` would quietly
treat any part with designator R as a resistor.

### 2.3 Parsing and canonical form (Rust, `crates/core/src/value.rs`)

The canonical form uses engineering notation: a mantissa from 1 up to (not
including) 1000, at most three significant digits where possible, and an SI
prefix, with no unit and no spaces. The display form adds a space and the
unit.

**Resistance** accepts:

- plain numbers and SI prefixes: `4700`, `4.7k`, `4.7K`, `2.2M`, `1G`
- RKM notation, where the letter stands in for the decimal point: `4k7`,
  `4R7`, `2M2`, `R47`
- an optional unit: `Ω`, `ohm`, `ohms`, `R` (trailing)
- `0`: a zero-ohm link

Prefixes are case-sensitive where it matters: `M` is mega and `m` is milli.
`m` is rejected for resistance as an almost-certain typo for `M`, with a
message that says so.

Examples: `4k7` → `4.7k` (`4.7 kΩ`); `10000` → `10k`; `0R47` → `470m`
(`470 mΩ`).

**Capacitance** accepts:

- SI prefixes `p n u µ m` and an optional `F`: `100n`, `0.1uF`, `4.7µ`, `22p`
- a 3-digit code, the number printed on ceramics: `104` → 10·10⁴ pF =
  `100n`. A bare three-digit integer is always read as a code, so `100` means
  10 pF. Because the normalised display (`10 pF`) is shown back immediately,
  a misreading is obvious and easy to fix.
- other bare numbers are rejected with "add a unit, e.g. 100n or 100p"

Invalid input for a resistance or capacitance part is an operation error,
not a warning. Values are short, typed by hand and checked on entry, so an
error is the clearest feedback. Values are limited to 32 characters for every
kind.

### 2.4 Resistor colour code

```rust
pub enum Band { Black, Brown, Red, Orange, Yellow, Green, Blue, Violet, Grey, White, Gold, Silver }
pub const DEFAULT_TOLERANCE: Band = Band::Brown; // 1%
pub fn resistor_bands(canonical: &str) -> Option<Vec<Band>>;
```

- **Five bands:** digit, digit, digit, multiplier, tolerance. Example: 10 kΩ =
  100 × 10² → brown black black red **brown**.
- **Multipliers:** black ×1 to white ×10⁹, gold ×0.1, silver ×0.01. The
  smallest value that can be coded is 1.00 Ω (100 × silver).
- **Zero ohms:** a single black band.
- **No bands** (`None`; the value is still shown as text):
  - values below 1 Ω, other than 0
  - values that need more than three significant digits
  - values that would need a multiplier beyond white

**Reading direction.** The tolerance band is brown, and the first digit is
often brown too, so the drawing has to show which end is which. As on real
parts, the bands are drawn as a group of four with a wider gap before the
tolerance band. The tolerance band goes on the right (or bottom) end of the
body as seen in the view, so the code reads left to right or top to bottom
whatever the part's rotation.

**Where the code lives.** The frontend computes bands too, because the Board
and Assembly scenes draw straight from the design. To keep Rust and
TypeScript from drifting, a shared fixture file
`crates/core/tests/fixtures/values.json` lists input → canonical → display →
bands. Both test suites run against it.

### 2.5 Operations

`change_instance_properties` replaces name, notes and properties wholesale.
It gets a new field:

```rust
#[serde(default)]
value: Option<String>,   // None = unchanged, Some("") = clear
```

It is `Option` so that existing callers, including MCP clients built against
the old schema, cannot wipe values just by leaving the field out. The
operation checks the value against the definition's `value_kind`, stores the
canonical form, and reports `Set R1 to 4.7 kΩ`.

### 2.6 Assembly plan

- **Step title:** `Fit R1 (1/4 Watt), 10 kΩ`.
- **Instructions:** a new line before "Insert…". For resistors:
  `Check the bands: brown, black, black, red, brown (10 kΩ, 1%).` For
  capacitors: `Check the marking: 100 nF (104).`, with the code given only
  when the value can be written as one.
- **`AssemblyStep`** gets `value: Option<String>` (the display form) and
  `bands: Vec<Band>`, so the UI does not parse instruction text.
- **`AssemblyPlan`** gets `parts: Vec<PartsListEntry>`, grouped by definition
  and value, natural-sorted, with the names of the parts in each group:

  ```
  1/4 Watt   10 kΩ    R1, R4, R7     (3)
  1/4 Watt   4.7 kΩ   R2             (1)
  Ceramic    100 nF   C1, C2         (2)
  ```

- **Step order does not change.** Reordering steps by value would fight the
  height and position priorities. The parts list covers "pull out every
  10 kΩ first".
- **Fingerprints:** the value is part of the step content, so changing R1 from
  10 kΩ to 4.7 kΩ clears that step's checkmark. This is intended: the part in
  the board is now wrong.
- **Info note:** a new info-level `assembly.value_missing` for resistance
  and capacitance parts with no value. It is never raised to a warning, and
  text-kind parts never trigger it.

### 2.7 Breadboard layout

`BreadboardPart` gets `value: Option<String>` (display form) and
`bands: Vec<Band>`. The layout algorithm itself does not change.

### 2.8 Rendering

**Board tab** (`frontend/src/board/BoardScene.tsx`)

- `partLabel` gets an optional second line holding the value, in a smaller
  `part-value` style. If both lines do not fit along the body, only the name
  is drawn and the value moves to the hover card, which always shows it.
- A new view toggle `partValues` (signal in `state/store.ts`, next to
  `pinLabels`), **off by default**. When it is off, the value still shows
  in the hover card.
- Print mode follows the toggle.
- There are no colour bands on this tab, which stays the design view.

**Assembly tab** (`frontend/src/assembly/`)

- The drawing shows bands on resistor bodies, on the component-side view only
  (the body is not visible from the solder side). Other parts with a value
  show it as text on the body.
- `resistor-standing` shows only its round footprint from above, so it gets
  text only on the drawing. The step panel shows the swatches.
- In the step panel, next to the title, the value is shown with a row of
  colour swatches, each with its colour name for accessibility (the name also
  goes in a `title` attribute and in screen-reader text).
- A new "Parts" section lists `plan.parts`.

**Breadboard tab** (`frontend/src/breadboard/`)

- Resistors are drawn with bands; other parts show their value as text.

**Shared code**

- `frontend/src/model/values.ts` holds the band computation for the canonical
  form, a display formatter, and a band-to-CSS-colour map.
- The colours are tuned so that brown, red and orange stay distinct, and
  black and white bands get a thin outline against the body colour.

**Inspector** (`frontend/src/panels/Inspector.tsx`)

- A "Value" field under Name with a placeholder from the kind (`e.g. 4k7`,
  `e.g. 100n`, `e.g. 1N4148`). The normalised display and the bands appear
  live under it.
- Server errors show inline on the field.

**Dev mock** (`frontend/src/dev/`)

- `mock-backend.ts`, `mock-assembly.ts` and `mock-breadboard.ts` are updated
  so the UI can be built without the Rust side.

### 2.9 MCP

No new tools are needed:

- `get_design` returns `value`.
- `get_assembly_plan` and `get_breadboard_layout` return `value`, `bands` and
  `parts`.
- `apply_operations` accepts `value` on `change_instance_properties`.

The tool descriptions and generated schemas are updated.

## 3. Slices

Each slice leaves the app runnable with its tests passing.

1. **Core model and parser.** Add `ValueKind` and `ComponentInstance.value`.
   Write `value.rs` (parse, canonicalise, display, `resistor_bands`, capacitor
   code). Add the shared fixture file and Rust tests: RKM forms, prefix case,
   the bare-number rules, boundaries (1 Ω, < 1 Ω, 0 Ω, 4 significant digits,
   the largest multiplier). Update the component format doc and the project
   format doc. Write ADR 0019 "Component values".
2. **Editing.** Add `value` to `change_instance_properties` (`Option`
   semantics) and update the built-in definitions. Add the Inspector field,
   the TS `values.ts` with tests against the shared fixtures, and the mock
   backend. Round-trip and history-encoding tests for the new field.
3. **Board tab.** Second label line, `partValues` toggle, hover card, print.
4. **Assembly.** Step `value`/`bands`, instruction line, `parts` list,
   `assembly.value_missing`, fingerprint test (changing a value un-checks a
   step). UI: swatches, bands on the drawing, Parts section.
5. **Breadboard.** `BreadboardPart` `value`/`bands` and drawing.
6. **MCP and docs.** Schema and tool descriptions, an MCP integration test
   that sets a value and reads the bands back, README editor guide, CHANGELOG,
   and screenshots.

## 4. Testing

- Rust unit tests for parsing and bands; the shared fixture table run by both
  Rust and Vitest.
- An operation test: invalid values are rejected with a message naming the
  part and the expected form; text values are stored verbatim (trimmed).
- Compatibility: a v2 project without values loads and saves byte-identical.
- Assembly and breadboard snapshot tests extended with valued parts.
- Visual check on WebKitGTK (not just Chromium), including print, for the
  label layout and the band colours.

## 5. Out of scope, possible later

- Per-part tolerance, which would bring 4-band codes back (gold 5%, silver
  10%). This is a property override plus a change to `resistor_bands`.
- Capacitor voltage rating and dielectric, and inductors (`ValueKind::Inductance`
  with the same machinery, including inductor colour codes).
- Checking values against E-series, e.g. warning that 4.8k is not E24/E96.
- Bulk-editing values, which is covered in part by batch edit (ADR 0017) if
  `value` is added there.
- Reading values from the existing notes.

## 6. Decisions

1. Values on the Board tab are off by default and turned on and off with the
   `partValues` view toggle.
2. A resistor or capacitor with no value gets only an info note
   (`assembly.value_missing`), never a warning.
