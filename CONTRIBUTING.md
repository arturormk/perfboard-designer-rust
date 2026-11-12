# Contributing

Thank you for your interest in Perfboard Designer. Issues and pull requests
are welcome.

## Before you start

- For anything larger than a small fix, open an issue first to agree on the
  approach.
- The product scope is in [the blueprint](docs/BLUEPRINT.md), and significant
  decisions are recorded as [ADRs](docs/adr/). A change that contradicts an
  ADR should propose a new ADR.

## Development

Setup is described in the [README](README.md#setup-and-development). Before
opening a pull request, run the quality checks listed there; CI runs the same
commands.

## Commits

- Use [Conventional Commits](https://www.conventionalcommits.org/) subjects:
  `feat(editor): …`, `fix(ui): …`, `docs: …`, `perf(board): …`.
- Explain *why* in the body when it is not obvious from the diff.
- Keep `main` linear (rebase rather than merge): the development version's
  patch number is the commit count of `main`.

## Versions

Do not edit version numbers in pull requests. Development versions are
computed automatically, and release versions are set by maintainers when
releasing. See [Versioning](README.md#versioning).

## License

By contributing, you agree that your contributions are licensed under the
[MIT License](LICENSE).
