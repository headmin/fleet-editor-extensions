# Changelog

## v0.2.0 (Unreleased)

### New: one generator verb — `flint gen`

Flint's generator face now lives under a single command:
`flint gen <kind> [--from <source>]`. The `--from` source's extension picks
the generator (`.pkg` → installed-check policy, `.sql` → policy around that
query, …); omitting `--from` emits a blank template where one exists.

| v0.1.x | v0.2.0 |
|---|---|
| `flint pkg X.pkg [--full\|--wire\|-o F\|--setup-experience]` | `flint gen software --from X.pkg [same]` |
| `flint app X.deb` (any format) | `flint gen software --from X.deb` |
| `flint pkg X.pkg --yml` | `flint gen software --from X.pkg --standalone` |
| `flint pkg X.pkg --update F` | `flint gen software --from X.pkg --update F` |
| `flint pkg X.pkg --policy [--apps\|--enforce\|--outdated-only\|--file P]` | `flint gen policy --from X.pkg [same]` |
| `flint pkg X.pkg --scripts DIR` | `flint gen scripts --from X.pkg -o DIR` |
| `flint query X.sql` | `flint gen query --from X.sql` |
| `flint query X.sql --policy` | `flint gen policy --from X.sql` |
| `flint profile X [--full\|--wire\|--regen-uuid\|-o]` | `flint gen profile --from X [same]` |
| `flint new profile\|fleet\|policy\|query\|label` | `flint gen <kind>` |
| `flint help-agents --install-skill` | `flint setup-agent` |

**Deprecations:** the legacy commands (`pkg`, `app`, `profile`, `query`,
`new`, and `help-agents --install-skill`) keep working unchanged — same
output, same exit codes — but are hidden from `--help` and print a
deprecation warning on **stderr** (stdout stays byte-compatible for
scripts). They are removed in v0.3.0.

### New: Fleet Maintained Apps + read-only instance views

- **`fma-slug` lint rule**: `slug:` and `fleet_maintained_app_slug:` values
  are validated against the FMA registry offline — unknown slugs now fail
  in CI with did-you-mean suggestions, not first at `fleetctl gitops` time.
- **`flint fma search|show|latest|refresh`**: slug lookup from the CLI;
  `refresh` pulls fmalibrary.com's feed into a local cache that lint,
  editor completions, and search all share.
- **`flint fleet status|software|fma|labels|teams`**: read-only instance
  views (GET-only client — it structurally cannot write). Connection reuses
  the `[fleet]` config with env/`.env` fallback and `op://` secrets.
- Standalone software files are named `<name>.package.yml` (derived from
  the installer) and print a copy-pasteable fleet-file `path:` reference.

### Fixes & improvements

- **Editor quick-fixes for structural rewrites**: multi-line fixes (e.g.
  expanding a directory `path:` entry into per-file entries with label
  propagation) now surface as LSP code actions — previously they were only
  available via `flint check --fix --unsafe-fixes`. Unsafe fixes are offered
  with a "(may change semantics)" caveat and are never the preferred action.
- **Unknown-key typo fix now auto-applies**: `flint check --fix` corrects
  unknown keys with an unambiguous closest match (e.g. `polcies:` →
  `policies:`); previously the fix was suggested but never applied.
- `flint check` accepts multiple path arguments (fixes the `flint-files`
  pre-commit hook), and `.fleetlint.toml [files].include` now actually
  narrows the linted set (previously it could only add).
- **`[thresholds]` is enforced**: `min_interval`, `max_interval`,
  `warn_select_star`, and `max_query_length` (new `query-length` warning)
  now flow from `.fleetlint.toml` into the rules — they were silently
  ignored. Defaults are unchanged. `warn_trailing_semicolon` is removed
  (it never gated any check; old configs still parse).
- **Brace glob patterns work**: `[files]` patterns like `fleets/**/*.{yml,yaml}`
  now match (previously they silently matched nothing). Config globs are
  compiled once instead of per-file.
- **~7× faster directory lint** on a ~120-file repo: each file is read and
  parsed once (the cross-file pass used to re-read everything), and report
  attachment is no longer quadratic.
- **LSP label diagnostics** now use the engine's repo index: Fleet's
  built-in labels (e.g. "macOS") no longer false-positive, and the
  diagnostic code is `label-reference` (was `unknown-label`).
- Missing-wrapper suggestions ("Place 'x' inside 'y'") are deterministic
  across runs (previously HashMap-order random among equally-valid wrappers).
- Rule doc links unified in one registry — coverage grew from 14 to 21
  rules; `list-rules --format json` drops the never-enforced
  `preview`/`severity` keys.

### Internal

- One shared fix applier (`flint_lint::fix`) behind `--fix`, `paths --fix`,
  and LSP quick-fix actions — previously three divergent implementations,
  and one typed `Fix` enum replacing four loose fields on `LintError`.
- `Span {line, column, len}` diagnostic locations; one shared YAML walker
  and line-finder set (was three per-rule copies); one levenshtein and one
  path normalizer (was three each across the workspace).
- Rule-code registry (`flint_lint::codes`) with completeness guard tests —
  a typo'd rule code can no longer ship silently.
- The CLI's 3,800-line `main.rs` split into per-command modules; installer
  metadata acquisition and GitOps repo discovery moved into the engine crate
  alongside the types they produce; the AI-agent reference generator
  (`help_agents`) moved to the CLI, dropping `clap` from the library.
- Library API surface curated: 13 public modules (was 25) plus root
  re-exports; decorative/dead trait methods and functions removed.
- Workspace-level dependency table; unused dependencies removed; hand-rolled
  glob engine replaced by `globset`.

## v0.1.4

Prior release — see git history.
