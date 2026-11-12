# ADR 0010: Odd/even versioning with commit-count development builds

- Status: accepted
- Date: 2026-11-12

## Context

The project is about to be published. Users, bug reports, and MCP clients
need to identify exactly which build they are running, while deliberate
releases should keep stable, hand-chosen numbers. Bumping a version file on
every commit is noisy and cannot embed the commit count of the commit that
contains it.

## Decision

1. **Scheme.** Versions are `v{major}.{minor}.{patch}`, and the parity of the
   minor number selects the line:
   - **Odd minor: development line.** The patch number is the number of
     commits reachable from `HEAD` (`git rev-list --count HEAD`). Every commit
     on `main` therefore has a distinct, increasing version, for example
     `v0.1.26`, `v0.1.27`. Nobody edits it.
   - **Even minor: release line.** The patch number is optional and only ever
     changed by hand: `v0.2.0`, then `v0.2.1` for a fix release.
2. **Single source of truth.** `[workspace.package] version` in the root
   `Cargo.toml` declares the line. On an odd line its patch must be `0` (the
   build fails otherwise); on an even line it is the release version.
   `frontend/package.json` mirrors the declared value, and
   `src-tauri/tauri.conf.json` has no `version`, so Tauri bundles use the Cargo
   one. A test in `perfboard-version` enforces both.
3. **Resolution in one place.** The `perfboard-version` crate resolves the
   effective version. Build scripts of the desktop app and the MCP server call
   `perfboard_version::emit_build_env()`, which sets
   `env!("PERFBOARD_VERSION")` and reruns only when `HEAD` moves. The
   `perfboard-version` binary prints the same value for scripts and CI
   (`--tag`, `--check-tag <tag>`).
4. **Where it shows.** The File menu footer, `get_bootstrap_info`, the MCP
   `serverInfo` (`perfboard-designer`), and `perfboard-mcp-server --version`.
5. **No Git metadata.** A development-line build without a repository fails
   with an explanation unless `PERFBOARD_COMMIT_COUNT` is set. Release-line
   builds never need Git.
6. **Tags.** Only release lines are tagged (`v0.2.0`). The release workflow
   refuses tags that are not an even minor equal to the workspace version.
   Development builds are identified by their version alone; the commit is
   the `N`th on `main`.

## Consequences

- CI and release checkouts need full history (`fetch-depth: 0`); a shallow
  clone would report a count of 1.
- The commit count is only meaningful on a linear `main`. Builds from other
  branches can share a number with a different `main` commit; that is
  acceptable for development builds and does not affect releases.
- Cargo metadata (`cargo metadata`, `CARGO_PKG_VERSION`) shows the declared
  `x.odd.0` on development lines. Code that reports a version must use
  `PERFBOARD_VERSION`.
- Cutting a release is two commits: set the even version and tag it, then set
  the next odd version so development builds resume counting.
