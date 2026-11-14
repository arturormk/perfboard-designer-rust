# Version 1 acceptance audit

Status: first pass, 2026-11-14; automated gaps closed the same day.

The development plan's Milestone 7 exit criteria require each of the
blueprint's 18 acceptance criteria ([BLUEPRINT.md §20](../BLUEPRINT.md)) to
have an automated test, a documented manual test, or both. This file maps
each criterion to its evidence and lists the gaps.

Automated tests are named `file::test`. Rust tests run with
`cargo test --workspace`; frontend tests with `npm --prefix frontend test`.
The manual steps refer to the [manual acceptance run](#manual-acceptance-run)
below, which also serves as the packaged-build smoke test.

## Summary

| # | Criterion | Automated | Manual | Verdict |
|---|---|---|---|---|
| 1 | Create a rectangular 2.54 mm perfboard project | yes | M1 | covered |
| 2 | Define or select about a dozen simple components | yes | M2, M3 | covered |
| 3 | Place and rotate components in a graphical editor | engine only | M4 | covered once M4 is run |
| 4 | See accurate pin positions, body dimensions, overhang and connector directions | yes | M4 | covered |
| 5 | Define the required electrical connections | yes | M5 | covered |
| 6 | Manually draw insulated wire routes | yes | M6 | covered |
| 7 | Ask the engine to suggest alternative routes | yes | M7 | covered |
| 8 | Use routing preferences suitable for hard wire | yes | M7 | covered |
| 9 | Favour available preformed U-jumpers | yes | M8 | covered |
| 10 | Inspect wire crossings and routing warnings | yes | M8 | covered |
| 11 | View both sides of the board | yes | M9 | covered |
| 12 | Compare committed and working designs | yes | M10 | covered |
| 13 | Undo changes and restore saved revisions | yes | M10 | covered |
| 14 | Let an LLM inspect and edit the same project through MCP | yes | M11 | covered |
| 15 | Read component Markdown documentation through MCP | yes | M11 | covered |
| 16 | Review and approve LLM-proposed electrical changes | yes | M11 | covered |
| 17 | Generate a useful wiring list and assembly plan | yes | M12 | covered |
| 18 | Save and reopen the complete project without losing its history | yes | M13 | covered |

No UI interaction is tested automatically: frontend tests cover models and
hit-testing, and the engine tests cover every operation the UI sends. The
manual run is what shows the graphical workflow works, so it must be run on
the release candidate.

## Evidence by criterion

### 1. Create a rectangular 2.54 mm perfboard project

- `crates/project/src/document.rs::empty_project_round_trips_without_loss`,
  `permanent_empty_fixture_matches_canonical_snapshot`,
  `invalid_pitch_is_rejected_at_boundary`
- `crates/core/src/geometry.rs::dimensions_count_intervals_not_holes`
- `frontend/src/editor/board-size.test.ts` (6 tests)

### 2. Define or select about a dozen simple components

- Format and validation: `crates/core/src/component.rs` (10 tests),
  including `designators_are_short_runs_of_letters`
- Part designer drafts: `crates/core/src/component_draft.rs` (5 tests);
  load, save, conflicts and backups: `src-tauri/src/component_viewer.rs`
  (10 tests); `frontend/src/component-designer/*.test.ts`
- Library resolution and grants: `crates/library/src/tests.rs`,
  `src-tauri/src/component_library.rs`, `src-tauri/src/library_grants.rs`,
  `src-tauri/src/library_parts.rs`
- The standard library: `src-tauri/src/component_library.rs::embeds_every_checkout_component_without_diagnostics`,
  (all 46 standard parts load cleanly and are compiled in),
  `crates/library/tests/generated_parts.rs` (the 41 generated parts validate
  and match their folders)

### 3. Place and rotate components in a graphical editor

- `crates/project/tests/milestone1.rs::place_move_rotate_save_and_reopen_example_components`
- `crates/core/src/geometry.rs::rotations_are_integer_and_clockwise`
- `frontend/src/editor/component-placement.test.ts`,
  `frontend/src/editor/drag.test.ts`, `frontend/src/board/hit.test.ts`

### 4. See accurate pin positions, body dimensions, overhang and connector directions

- Pins and bodies: `crates/core/src/geometry.rs::component_view_uses_canonical_coordinates`,
  `crates/project/src/rendering.rs::rendering_is_deterministic_and_escaped`
- Overhang: `crates/project/tests/assembly.rs::a_body_over_another_parts_pins_goes_in_after_it`
- Connector directions (`access` regions):
  `crates/core/src/validation.rs::required_access_must_stay_on_the_board_unless_it_may_overhang`,
  `required_access_follows_rotation_and_is_blocked_by_other_parts`. The board
  draws them in `frontend/src/board/BoardScene.tsx`; seeing them is M4.

### 5. Define the required electrical connections

- `crates/project/tests/milestone2.rs::conflicting_pin_assignment_is_atomic`,
  `nets_can_be_renamed_to_unique_non_empty_names`
- `crates/project/tests/merge_nets.rs` (2 tests)
- `crates/project/src/connectivity.rs::net_connectivity_groups_pins_into_islands`
- `frontend/src/editor/net-pins.test.ts`, `frontend/src/model/ratsnest.test.ts`

### 6. Manually draw insulated wire routes

- `crates/project/tests/milestone2.rs::permanent_example_contains_four_complete_manual_routes`
- `crates/project/tests/milestone4.rs::routes_and_manual_edits_cannot_pass_through_an_unrelated_pin`,
  `reconnecting_a_wire_endpoint_repairs_its_route_and_net_assignments`
- `frontend/src/editor/wire-routing.test.ts` (19 tests)

### 7. Ask the engine to suggest alternative routes

- `crates/project/tests/milestone4.rs::identical_state_and_config_produce_byte_equivalent_obstacle_avoiding_routes`,
  `router_uses_only_orthogonal_moves`,
  `required_bend_spacing_and_no_path_explanations_are_enforced`
- `frontend/src/editor/net-routing.test.ts`
- Alternatives: `suggest_route_alternatives` offers up to five distinct
  routes, the balanced one first, then a different path (off the balanced
  route's holes), fewest bends, shortest and the other side, dropping
  repeats. The UI shows them as tabs on the route card (keys 1–5); MCP's
  `suggest_routes` returns them with `alternatives: true`.
  `crates/project/tests/milestone4.rs::alternatives_lead_with_the_balanced_route_and_never_repeat`,
  `a_straight_connection_offers_a_detour_and_the_other_side`,
  `a_required_side_rules_out_the_other_side_alternative`;
  `crates/session-ipc/src/lib.rs::route_alternatives_are_optional_on_the_wire`

### 8. Use routing preferences suitable for hard wire

- `crates/project/tests/milestone4.rs::router_uses_only_orthogonal_moves`,
  `router_avoids_an_equally_short_run_along_the_board_edge`,
  `required_bend_spacing_and_no_path_explanations_are_enforced`,
  `constraint_crud_is_revisioned_and_required_rules_survive_manual_override`
- `crates/core/src/routing.rs::counts_direction_changes_not_collinear_vertices`

### 9. Favour available preformed U-jumpers

- `crates/project/tests/milestone4.rs::exact_and_tolerance_boundary_jumper_matches_are_deterministic`

### 10. Inspect wire crossings and routing warnings

- `crates/project/tests/milestone4.rs::crossings_are_explicit_and_do_not_create_connectivity`
- `crates/project/tests/milestone2.rs::crossing_wires_do_not_join_disconnected_terminal_groups`
- `crates/project/src/connectivity.rs::short_segment_warning_ignores_first_and_last_segments`,
  `component_clearance_uses_wire_keepouts_not_placement_keepouts`
- `crates/project/tests/assembly.rs::a_recorded_crossing_puts_the_lower_wire_first`

### 11. View both sides of the board

- `crates/core/src/geometry.rs::solder_view_mirrors_rectangular_board_edge_holes`,
  `solder_view_mirrors_half_grid_routes`, `mirroring_twice_returns_the_original_position`
- `crates/project/src/rendering.rs::solder_view_mirrors_geometry`
- `crates/project/tests/milestone4.rs::solder_side_routes_may_pass_beneath_a_component_body_but_not_its_pins`

### 12. Compare committed and working designs

- `crates/project/tests/milestone3.rs::semantic_diff_classifies_movement_and_rotation`

### 13. Undo changes and restore saved revisions

- `crates/project/tests/milestone3.rs::undo_redo_and_save_reopen_preserve_working_history`,
  `restore_keeps_descendants_and_creates_a_dirty_working_copy`,
  `redo_survives_failed_work_but_clears_after_a_divergent_edit`,
  `undo_history_can_be_trimmed_or_deleted_without_changing_the_design`,
  `failed_transaction_leaves_serialized_and_memory_state_unchanged`

### 14. Let an LLM inspect and edit the same project through MCP

- `crates/mcp-server/tests/milestone5.rs::real_stdio_server_reads_and_mutates_the_shared_session`
  (the real MCP binary over stdio, against a session host)
- `crates/project/tests/milestone3.rs::two_requests_at_one_revision_yield_one_success_and_one_structured_conflict`
- `crates/mcp-server/src/main.rs::tool_catalog_is_codex_and_claude_friendly`,
  `server_instructions_lead_with_revision_safety`

### 15. Read component Markdown documentation through MCP

- `crates/mcp-server/src/main.rs::tool_catalog_is_codex_and_claude_friendly`
  (the tool exists and is read-only)
- `src-tauri/src/component_library.rs::embeds_every_checkout_component_without_diagnostics`
  (standard parts carry their README)
- `crates/mcp-server/tests/milestone5.rs::real_stdio_server_reads_and_mutates_the_shared_session`
  calls `get_component_documentation` through the real MCP binary: the
  Markdown arrives marked `untrusted_reference`, and an unknown part is an
  error
- `src-tauri/src/lib.rs::agents_read_part_documentation_from_the_project_and_standard_libraries`
  (the desktop lookup, for a project-folder part and a standard part)

### 16. Review and approve LLM-proposed electrical changes

- `crates/project/tests/milestone2.rs::proposed_connectivity_requires_explicit_acceptance`
- `crates/project/tests/milestone4.rs::accepting_a_proposal_preserves_unrelated_and_locked_routes_and_round_trips`
- `crates/project/tests/merge_nets.rs::merging_rejects_self_missing_and_proposed_nets`

### 17. Generate a useful wiring list and assembly plan

- `crates/project/tests/assembly.rs` (14 tests), including
  `the_wire_list_covers_every_wire_with_estimates_and_steps` and
  `the_sample_project_gives_a_usable_sequence`
- `frontend/src/print/*.test.ts`, `frontend/src/assembly/scene.test.ts`

### 18. Save and reopen the complete project without losing its history

- `crates/project/tests/history_encoding.rs` (5 tests), including
  `history_round_trips_exactly` and `version_1_history_is_upgraded_on_load`
- `crates/project/tests/milestone3.rs::undo_redo_and_save_reopen_preserve_working_history`
- `crates/project/tests/milestone4.rs::sample_project_auto_route_manual_adjust_and_reopen_has_no_route_drift`
- `crates/project/src/persistence.rs::save_and_reopen_preserves_document`

## Gaps to close

1. ~~Access regions (criterion 4).~~ Tested.
2. ~~Alternative routes (criterion 7).~~ Implemented and tested.
3. ~~Documentation through MCP (criterion 15).~~ Tested end to end.
4. **Run the manual acceptance run** below on the release candidate and
   record the result here.

## Manual acceptance run

Run on the packaged build, from a clean user account or container, with the
MCP adapter registered in an MCP client (see the README). Record the build
version, the package type, and any step that fails.

- **M1. Create.** File → New board…: a 25 × 18 board. It is drawn as a
  rectangle of 25 × 18 holes at 2.54 mm.
- **M2. Standard parts.** The palette lists the standard library: resistors,
  capacitors, diodes, LEDs, TO-92, TO-220, DIP packages, headers and screw
  terminals.
- **M3. A part of your own.** Save the project. In Part designer, use **New
  part here** in the project folder, give it two pins, a name and a
  designator, and save. It appears in the palette marked as a project part.
- **M4. Place and rotate.** Place about a dozen parts, including a DIP-8, an
  electrolytic, a screw terminal and your own part. Rotate some with **R**
  while placing and after. Names follow designators (R1, C1, U1, …). Bodies,
  pins, occupied holes and the screw terminal's wire-entry arrow are drawn
  where expected; moving a part onto another's pins or keepout is refused or
  reported.
- **M5. Connect.** Create nets and add pins to them (Alt + click a pin
  toggles it on the active net). Rename and merge a net.
- **M6. Draw wires.** With the Wire tool (**W**), draw an insulated wire with
  bends between two pins of one net; remove a bend with Backspace while
  drawing.
- **M7. Suggest routes.** Ask for a suggested route for an open connection.
  The route card offers alternatives as tabs; preview each with 1–5, then
  accept one with Enter. Add a routing rule (for
  example bend spacing) and see the next suggestion respect it.
- **M8. Jumpers, crossings and warnings.** Add a U-jumper to the jumper
  inventory and route a connection of its length: the suggestion uses it.
  Make two wires cross on the same side: the crossing is shown and listed in
  Problems, and choosing which goes on top is recorded.
- **M9. Both sides.** Flip to the solder side (**F**): the board is mirrored,
  and solder-side wires are drawn on it.
- **M10. History.** Commit. Move a part and delete a wire, then compare with
  the commit: the differences are shown. Undo both changes. Make another
  change, commit, then restore the first commit.
- **M11. MCP.** From the MCP client: read the project, read a part's
  documentation (`get_component_documentation`), place a part and propose a
  new net. The desktop app shows the agent's edit; the proposed net waits in
  Nets for acceptance; accept it.
- **M12. Assembly.** Switch to Assembly: the wire list and build order are
  complete and in a sensible order. Print the checklist and wire templates.
- **M13. Save and reopen.** Save, quit, reopen the file: the design, commits,
  undo history and assembly progress are as they were.
