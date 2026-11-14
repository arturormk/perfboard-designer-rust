# ADR 0014: Layered component libraries and user-library grants

- Status: accepted
- Date: 2026-11-14

## Context

The palette loads a single directory: the bundled `components/`, found
through the Tauri resource directory or, failing that, a path compiled into
the binary (`CARGO_MANIFEST_DIR/../components`). That fallback is correct in a
checkout and wrong for any binary copied elsewhere. MCP does not read that
directory at all: `list_components` and `get_component_definition` return
three definitions embedded with `include_str!`. The palette and MCP therefore
disagree about what can be placed.

The bundled directory mixes two kinds of part. `bridge`, `dupont-jumper`,
`dupont-pin` and the two resistors are generic; `arduino-nano-like`, both
`rs485-breakout`s and `rj45-connector` exist for the sample project. An
installed package makes the directory read-only, so a user has nowhere to keep
parts of their own.

Project files are already portable: every placed instance carries an embedded
snapshot of its definition (project format v2, `component_definitions`). What
needs deciding is which definitions the user, and MCP acting through the
desktop session, can *add* to a project later. A project received from someone
else should not, by being opened, gain access to folders on the recipient's
machine.

## Decision

### Layers

A project's component library is an ordered list of layers. The palette and
MCP read the same list through one resolver in a shared crate, not in
`src-tauri`.

1. Project layers and user-library layers, in the order the project lists them.
2. The standard library, always last. It cannot be removed or reordered.

The first layer that defines an `id` wins. A definition hidden by an earlier
layer is reported as shadowed in the palette, so that a project part
overriding a standard one is visible as such. Two definitions with the same
`id` in one layer are a diagnostic on that layer, and neither is loaded.

Resolution affects only new placements and "newer definition available"
checks. Placed instances keep their embedded snapshots, so no change to
layers, grants or the standard library alters an existing project.

### Paths in the project file

Projects gain an optional top-level field:

```json
"component_paths": [
  "${PROJECT_DIR}/${PROJECT_STEM}.perfboard",
  "${USER_COMPONENTS}/rs485",
  "${USER_COMPONENTS}"
]
```

- Every entry starts with `${PROJECT_DIR}` (the folder holding the project
  file) or `${USER_COMPONENTS}` (see the next section), optionally followed by
  a relative path. `${PROJECT_STEM}` (the file name without
  `.perfboard.json`) may appear after the root.
- No other variables are expanded. Absolute paths, `~`, `..` and empty
  segments are rejected when the project loads and when the field is edited.
- Each resolved layer is canonicalized and must stay under the canonical root
  it names. A symlink inside a project folder therefore cannot reach outside
  it.
- Without the field, the list is `["${PROJECT_DIR}/${PROJECT_STEM}.perfboard"]`.
  The example `examples/nano-rs485.perfboard.json` therefore uses
  `examples/nano-rs485.perfboard/`.
- The field is outside `design` and outside undo history. Like the project
  name, it is a project setting: changing it marks the project as unsaved.
  It needs no schema version change. Older builds ignore the field and drop it
  when they save.
- An unsaved project has no `${PROJECT_DIR}`. Its project layers are inactive
  until the first save.

### User-library grants

`${USER_COMPONENTS}` has no default. It is defined per project by a grant file
on the user's machine, never stored in or next to the project:

```
$XDG_CONFIG_HOME/perfboard-designer/projects/<project_id>.toml
```

```toml
user_components = "/home/arturo/electronics/parts"
# The two fields below are for display in Preferences only.
project_name = "Nano RS-485 controller"
last_seen_path = "/home/arturo/work/nano-rs485.perfboard.json"
```

Keying grants by `project_id` means a grant cannot travel with the project
when it is zipped, emailed or committed, and survives renaming or moving the
project file. Someone who receives a project sees its project layers and the
standard library, nothing of the sender's machine or their own user library,
until they grant access themselves. Copying one's own project to a second
machine needs one re-grant there.

`user_components` must be an absolute path to an existing directory. The
directory may be shared by any number of projects.

### Missing layers and errors

- A project entry that uses `${USER_COMPONENTS}` while the project has no
  grant, or whose grant points to a missing directory, is an error on that
  layer. It is shown in the libraries panel and reported by validation. The
  project still opens, edits and saves normally: only new placements from
  that layer are unavailable.
- An explicitly listed `${PROJECT_DIR}` layer whose folder does not exist is
  a warning. The default entry, used when `component_paths` is absent, is
  silently empty when its folder does not exist.
- A definition that fails to parse is a diagnostic on its layer, and the rest
  of the layer still loads.

### Configuration in the UI

Grant files and `component_paths` are written only by Tauri commands invoked
from the UI. The MCP session bridge (ADR 0007) gets read access to resolved
layers and no request that changes paths or grants.

- **Project settings → Component libraries:** the ordered layer list with
  each entry's template, resolved path, status (loaded, missing, not granted,
  read-only) and part count. Entries can be added, removed and reordered,
  except the standard library. A folder picker turns the chosen folder into a
  template relative to the project folder or the granted user library, and
  refuses folders under neither. The user-library row offers **Grant…**,
  **Change…** and **Revoke**. Grant opens a folder picker, checks the folder
  and writes the grant file. Granting works before the first save, because
  `project_id` exists from creation.
- **Preferences → Granted projects:** every grant file, listed by project name
  and last-seen path, with **Revoke**, so grants for deleted or moved projects
  can be cleaned up.

### The standard library

The standard library is compiled into the binary. A `build.rs` step embeds
every `components/*/component.toml` and `README.md`, with `rerun-if-changed`
on `components/`. The standard library is part of the application version
(ADR 0010).

- Packaged builds (.deb, .rpm, AppImage) and hand-copied binaries all read
  the embedded copy. There is no discovery and nothing to install alongside
  the binary. The `components/` Tauri resource and the resource-directory
  lookup are removed.
- `PERFBOARD_STANDARD_COMPONENTS=/path`, when set, replaces the embedded copy
  with a directory. It is meant for library authors, packagers and CI.
- Debug builds default to the checkout's `components/`, so a standard part
  edited in the repository appears without rebuilding. Release builds never
  read build-time paths.
- The palette names the active source: "Standard library (built-in, vX.Y.Z)"
  or the override path.

### Editing and saving

The component editor saves only into a writable project layer or a granted
user layer. To change a standard part, the user copies it into one of those.
Copying keeps the `id`, so the copy shadows the standard part. ADR 0008's
save procedure (revision check, backup, atomic replace) applies unchanged.
The editor's authorized root (`AuthorizedComponent`) becomes the layer
being written to.

### Identifiers

The `dev.perfboard.` prefix is reserved for the standard library. Outside it:

- shadowing a standard part with the same `id` is allowed and reported as
  shadowing;
- any other `dev.perfboard.*` `id` gets a warning, because a future standard
  part could take it.

Project and user parts should use the author's own reverse-DNS prefix, or
`local.` when they have none. The sample project's four specific parts move to
`examples/nano-rs485.perfboard/` and are renamed out of `dev.perfboard.`,
together with their instances and snapshots in the example project. The
namespace is frozen at 1.0.

## Consequences

- The palette and MCP list the same definitions for a given project, closing
  the three-definition gap.
- Opening a project from someone else never reads folders outside the project
  folder or the standard library unless the recipient grants access.
- A user library is opt-in per project. A user with many projects grants each
  one once. The folder picker remembers the last granted folder to make that
  quick.
- A fix to a standard part needs an application release. Until then the
  corrected part can live in a user library and shadow the standard one.
- Grant files are per machine and are not backed up with the project. Losing
  one costs a re-grant, never data.
- Opening and saving a project in an older build drops `component_paths`.
  Builds older than this ADR are pre-1.0, so this is accepted.
