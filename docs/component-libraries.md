# Component libraries: writing and organising parts

This guide is for anyone who makes parts: for one project, for all of their
own projects, or for the standard library that ships with the application.
The TOML fields themselves are described in
[the component format](format/component-v1.md); the reasons behind the
library rules are in [ADR 0014](adr/0014-component-library-layers.md).

## Where parts come from

The palette and MCP agents offer the parts of the open project's **libraries**,
searched in order:

1. **Project folders.** By default, the folder named after the project file:
   `board.perfboard/` beside `board.perfboard.json`. Parts made for one board
   live here and travel with it.
2. **Your user library.** A folder of your own parts that several projects can
   share, such as `~/electronics/parts`. A project sees it only after you grant
   it access (see [User libraries and grants](#user-libraries-and-grants)).
3. **The standard library.** General-purpose parts built into the application:
   resistors, capacitors, diodes and LEDs, TO-92 and TO-220 packages, DIP
   packages, pin headers, screw terminals, the bridge and the Dupont jumper
   and pin. Always searched last, and read-only.

Each library folder holds one part per subfolder:

```text
board.perfboard/
  rs485-breakout/
    component.toml
    README.md        (optional)
  rj45-connector/
    component.toml
```

Subfolders without `component.toml` and hidden folders are skipped. Only one
level is read: a folder of folders is not searched.

**The first library that has a part's `id` wins.** A project part with the same
`id` as a standard part replaces it for that project, and the palette marks it
as overriding. Two parts with the same `id` in one library are an error, and
neither loads.

### Choosing the folders

**File → Component libraries…** lists the project's libraries in search order.
From there you can:

- **Add folder…**: pick a folder inside the project's folder or inside your
  granted user library. Folders anywhere else are refused.
- **Add user library**: search the granted user library.
- Reorder or remove entries. The standard library always stays last.
- **Reset to default**: search only `<project name>.perfboard/`.

The list is saved in the project file as `component_paths`:

```json
"component_paths": [
  "${PROJECT_DIR}/${PROJECT_STEM}.perfboard",
  "${USER_COMPONENTS}/rs485",
  "${USER_COMPONENTS}"
]
```

Every entry starts with `${PROJECT_DIR}` (the folder holding the project file)
or `${USER_COMPONENTS}` (the granted user library), optionally followed by a
relative path. `${PROJECT_STEM}`, the file name without `.perfboard.json`, may
appear after the root. Absolute paths, `~`, `..` and other variables are
rejected, and a link inside a library cannot point outside it. A project
therefore never names a place on your computer other than its own folder and
whatever you have granted it.

An unsaved project has no folder yet, so its project folders are inactive
until the first save.

## User libraries and grants

A grant tells one project which folder `${USER_COMPONENTS}` means **on this
computer**. Grants are not stored in the project. They are kept in the
application's configuration folder, on Linux
`~/.config/dev.perfboarddesigner.desktop/library-grants/`, one file per
project.

- **Grant**: in **File → Component libraries…**, under *User library on this
  computer*, choose **Grant…** and pick the folder. **Change…** picks another
  one, and **Revoke** removes the grant. You can grant before the first save.
- **Review**: **File → Preferences…** lists every grant with the project's name
  and where it was last opened, so you can revoke grants of projects you have
  deleted or moved.
- **Sharing**: a project you send to someone carries no grant. They see its
  project folders and the standard library, nothing from your computer, until
  they grant a library of their own. Copying your own project to a second
  computer needs one grant there too.

Granting alone does not add the user library to the search. The project's
libraries must also list a `${USER_COMPONENTS}` entry: use **Add user
library**. A listed but ungranted user library is reported in the Problems
panel; the project still opens, edits and saves.

## Making and changing parts

Switch to **Part designer**. It opens on the list of the project's parts,
library by library.

- **Open a part**: click it. Parts in project folders and the user library
  open for editing.
- **New part here**: creates `new-part/` in the first project folder or user
  library, with the `id` `local.new-part`. Set the name, `id` and designator
  in *Identity* before saving.
- **Change a standard part**: use **Copy to project** or **Copy to user
  library**. The copy keeps the `id`, so it replaces the standard part for
  this project, and opens for editing.
- **Delete a part**: the trash button beside it, shown on hover. The folder
  goes to the system trash, where your file manager can restore it.
- **Open part folder…** opens any part folder, or lists the parts of a folder
  that holds several. Use it for libraries that are not part of the project,
  such as when working on a library of your own.

Saving checks the definition first and refuses to save an invalid one. It
also refuses when `component.toml` or `README.md` changed on disk since the
part was opened. The previous files are kept as `component.toml.bak` and
`README.md.bak`.

`README.md` is shown to people and to MCP agents as documentation for the part.
Agents treat it as reference text, never as instructions.

## Identifiers

### `id`

The `id` is how projects and libraries recognise a part. Choose it once and
keep it.

- **Standard library**: `dev.perfboard.` is reserved for it.
- **Your parts**: use a reverse-DNS prefix you control, such as
  `com.example.nano-like`, or `local.` when you have none:
  `local.rs485-breakout`.
- Any other `id` starting with `dev.perfboard.` gets the `library.reserved_id`
  warning, because a future standard part could take it. Copying a standard
  part keeps its `dev.perfboard.` `id` on purpose, and does not warn.

### `definition_version`

Raise it whenever you change a part, for example `1.0.0` to `1.1.0`. The
inspector shows *library has 1.1.0* on placed parts with an older version.

### `designator`

The reference designator prefix, such as `R`, `C`, `D`, `Q`, `U`, `J`, `JP` or
`W`. New parts are named with it and the lowest free number: `R1`, `R2`, …
Without one they are named after the part's name: `Bridge 1`.

### Pin `id` and `number`

Pin `id`s are what nets and wires refer to; `number` is the marking on the
part. Keep both stable once a part has been placed anywhere.

## What changes do to placed parts

A project stores a copy of every definition it uses, so changing or deleting a
library part never changes a project by itself.

- **Metadata changes** (name, designator, description, documentation,
  `definition_version`) reach a project when you place the part again or
  choose **Update metadata** in the inspector. Names of parts already placed
  stay as they are.
- **Shape changes** (pins, body, holes, keepouts, rotations, kind) cannot
  replace a definition that a project already uses: placing the changed part
  there is refused. If projects that use the old part should take the new one,
  give it a new `id`, such as `local.rs485-breakout.2`, and keep the old part
  until no project needs it.

## Library problems

Library problems appear below the palette, in the Problems panel and in MCP's
validation report. They never block opening, editing or saving a project; they
only make parts unavailable for new placements.

| Code | Meaning |
|---|---|
| `library.not_granted` | The project lists `${USER_COMPONENTS}` but has no grant on this computer. |
| `library.user_components_unavailable` | The granted folder no longer exists. |
| `library.layer_missing` | A listed project folder does not exist (warning). The default `<project name>.perfboard/` is silently empty instead. |
| `library.project_dir_unavailable` | The project has not been saved, so its folders do not exist yet. |
| `library.invalid_path` | A `component_paths` entry breaks the rules above. |
| `library.layer_outside_root`, `library.component_outside_root` | A link points outside the library's root. |
| `library.layer_not_directory`, `library.layer_unreadable` | A listed folder is a file or cannot be read. |
| `library.duplicate_layer` | The same folder is listed twice (warning). |
| `library.duplicate_id` | Two parts in one library share an `id`; neither loads. |
| `library.invalid_definition`, `library.component_unreadable` | A `component.toml` does not parse, fails validation or cannot be read. The rest of the library still loads. |
| `library.documentation_unavailable` | `README.md` exists but cannot be read (warning). |
| `library.reserved_id` | A part outside the standard library uses `dev.perfboard.` (warning). |
| `library.standard_unavailable` | The standard library folder set with `PERFBOARD_STANDARD_COMPONENTS` cannot be read. |

## MCP agents

Agents read the same libraries as the palette: `list_components`,
`get_component_definition` and `get_component_documentation` return the open
project's resolved parts. They cannot change `component_paths` or grants, and
they cannot create or edit parts. Agents that add a part with an empty name get
the next designator, as in the UI.

## Contributing to the standard library

Standard parts live in `components/` in the repository and are compiled into
the application.

- Use `dev.perfboard.` `id`s and keep them stable: the namespace is frozen
  at 1.0.
- Debug builds (`cargo tauri dev`) read `components/` directly, so edits show
  without rebuilding. `PERFBOARD_STANDARD_COMPONENTS=/path` points any build at
  another folder.
- Only general-purpose parts belong here. Parts for one project go in its
  project folder, as the sample project's do in
  `examples/nano-rs485.perfboard/`.
- `cargo test --workspace` checks that every part in `components/` loads
  without problems and is compiled in.
- Part families that come in many sizes (headers, DIP packages, capacitors,
  diodes, LEDs, TO-92 and TO-220, screw terminals) are generated by
  `crates/library/tests/generated_parts.rs`. Their `component.toml` starts
  with a `# Generated by …` line: change the generator, not the file, then
  rewrite them with
  `PERFBOARD_REGENERATE_PARTS=1 cargo test -p perfboard-library --test generated_parts`.
  The test fails while a generated folder is out of date, and the generator
  marks every hole under a part's body, other than its pins, as occupied and
  kept out.
