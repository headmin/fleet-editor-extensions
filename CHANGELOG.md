# Changelog

Milestones per release. Full notes are on each
[GitHub release](https://github.com/headmin/fleet-editor-extensions/releases).

## v0.3.4 (unreleased)

- Rule levels per path: `[rules] error`, `[[overrides]]`, `extend` for a shared base config, and `check --error/--warn/--ignore`.
- `unused-ignore-comment` reports `# flint: ignore` comments that suppress nothing; `check --add-ignore` accepts today's findings as a baseline.
- `flint init` no longer turns off unknown-key detection; three settings that never did anything are flagged.
- Editors pull diagnostics where supported, and gitops dry-run results no longer replace lint findings.
- A JSON Schema for `fleetlint.toml` is published with the docs; `--fix` reports what is left; the docs' examples are tested.
- Key completions insert `key: ` (or an indented section), fix `-key` to `- key`, and say where a key lives; keys are coloured by where they are, misplaced ones as invalid; a section moved to the margin, or lines indented under a value-only key, are flagged where they are.
- Completion suggests the key names and paths your target Fleet version and repo layout expect (`reports`, `apple_settings`, `../platforms/`), every key the schema accepts at that spot and never one it rejects; the software snippets no longer lose their indentation.
- `flint fma refresh` regenerates the full FMA list from Fleet as an override file; `[fma] registry` keeps it in the repo (#11).

## v0.3.3 (2026-10-23)

- Release builds pin patched TLS crates (rustls-webpki 0.103.16, webpki-roots 1.0.11): emSign roots are no longer trusted.
- Policies accept `continuous_automations_enabled`; `software.packages[].configuration` and `controls: path:` are accepted; the removed `conditional_access_bypass_enabled` is flagged.
- 19 keys Fleet's parser rejects are now errors (e.g. Jira credentials in a fleet file, `require_all_software`, `policies[].version`); files using them already failed `fleetctl gitops`.
- The schema is tested against Fleet's own key list in both directions, so a missing or invalid key fails the build.

## v0.3.2 (2026-10-16)

- `flint census`: compare settings across fleets, with `--anchor`, `--fleet` and `--key`.

## v0.3.1 (2026-10-10)

First published 0.3 release; includes v0.3.0.

- Schema follows Fleet v4.93.0.
- `flint check --fleet <glob>` reports on one fleet; single-file and sub-directory checks run from the repo root.
- Patch policies are checked against the fleet's Fleet-maintained apps; invalid YAML is an error in every file kind.
- `--fix` escapes an unresolvable `${VAR}` that appears only in a comment.

## v0.3.0 (not published separately)

- `flint history`: replay today's rules against past commits (`--suggest-patterns`, `--oracle`, `--gate`).
- New rules: `profile-well-formed`, `payload-uuid-format`, `duplicate-fleet-name`.
- `unregistered-script` is scoped per fleet; profile rules run repo-wide.
- Legacy generator commands (`flint pkg`, `query`, `new`, `profile`, `app`) removed; use `flint gen` (mapping table in [docs/docs/commands.md](docs/docs/commands.md)).

## v0.2.2 (2026-08-13)

- `secret-hygiene` checks URL query strings.
- `flint dry-run --refresh-snapshot` and `--assume-uploaded`; `flint fleet doctor`.

## v0.2.1 (2026-08-11)

- Schema follows Fleet v4.90.0; every key Fleet resolves as a path is checked as a file reference.

## v0.2.0 (2026-08-09)

- One generator command: `flint gen <kind> [--from <source>]`.
- Fleet-maintained apps: `fma-slug` rule and `flint fma`; read-only `flint fleet` views.
- Editor quick-fixes for structural rewrites; about 7× faster directory lint.

## v0.1.4 (2026-08-06)

Earlier releases: see git history.
