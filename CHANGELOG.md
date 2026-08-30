# Changelog

## v0.3.0 (Unreleased)

### Removed: the legacy generator spellings

As announced in v0.2.0, `flint query`, `flint new`, `flint profile`,
`flint app`, `flint pkg` and `flint help-agents --install-skill` are gone. They
had forwarded to `flint gen …` (and `flint setup-agent`) with a warning on
stderr; the mapping table under v0.2.0 below is the migration guide. An old
spelling now fails as an unrecognized subcommand — deliberately loud, so a
script that still uses one breaks at the first run rather than drifting.

### New: `flint history` — what the rules would have caught

Replays today's rules against past commits, so rule priority rests on measured
recurrence rather than judgement. Each first-parent commit is reconstructed with
`git archive` into a scratch directory — the working copy is never touched — and
linted. Findings are grouped into *red windows*: runs of commits in which a code
fired, and the commit that closed each one.

```
flint history --max 400                  # replay; red windows per rule
flint history --suggest-patterns         # mine remediation commits for guardrails
flint history --oracle <gitops-oracle>   # diff against Fleet's own parser
flint history --gate <baseline.json>     # fail CI when rule quality regresses
```

Only **closed** windows score. A finding still present at HEAD describes current
state, not a repeated mistake, and must not inflate a recurrence count.

`software-source` is excluded and says so: a snapshot-derived finding depends on
the Fleet server's state at that commit, which is gone.

`--oracle` puts each tree through `spec.GitOpsFromFile` — the function
`fleetctl gitops` itself calls — and diffs blocking claims both ways: rules that
say an apply will fail where Fleet accepts, and Fleet complaints flint is silent
on. Dev/CI only; the oracle is not shipped.

`--gate` compares a run to a stored scorecard and exits 2 on regression. Only new
*keys* gate — occurrence counts move with the range replayed, so they are
reported and never failed on.

### New rules

- **`profile-well-formed`** (error) — a `.mobileconfig` or DDM declaration that
  does not parse. Dependency-free: no `plutil` (macOS-only), no new crate.
  Existed because `parse_mobileconfig` is a hand-rolled scanner that *cannot*
  fail, so one raw `&` in a brand name silently emptied every profile-level rule
  for that file while `fleetctl gitops` refused the apply. Binary plists and
  signed DER profiles are reported as info rather than skipped.
- **`payload-uuid-format`** (warning) — a `PayloadUUID` that is not 8-4-4-4-12
  hexadecimal. Registers **no** fix and is never auto-rewritten: a PayloadUUID is
  part of profile identity, so changing one makes Fleet re-deliver that profile to
  every enrolled host.
- **`duplicate-fleet-name`** (error) — two fleet files declaring the same `name:`.
  They do not conflict; Fleet collapses them server-side, the second silently
  wins, and a team ceases to exist.

### Changed — these may newly fail a previously green run

- **`unregistered-script` is now scoped per fleet.** It accepted a script
  registered by *any* fleet, so one declared in fleet A satisfied a policy applied
  by fleet B. `fleetctl` validates per team; flint now matches. Expect findings
  wherever a policy file is pulled into several fleets by a glob and the script is
  registered in only some — those are real apply failures.
- **`path-exists` carries the missing target in `related`**, so
  `flint check --staged` blocks the commit that *deletes* a referenced file.
  Previously those findings sat on unstaged fleet YAML and were demoted to
  "pre-existing elsewhere".
- **`duplicate-identifier` and `duplicate-content` compare a canonical form**
  rather than raw bytes. Two profiles differing only in XML escaping (`&quot;`
  against a literal `"`) decode identically, so byte comparison both invented
  divergences and hid real duplicates. `duplicate-content` now distinguishes the
  two cases in its message.
- **Profile-level rules run repo-wide, not per fleet file.** A profile reached by
  N fleets' globs now yields one finding instead of N, and a profile nothing
  references is checked at all. Consequence: these rules no longer run on a
  single-file lint (`flint check one.yml`), only on a directory lint.

## v0.2.2 (2026-08-13)

- **`secret-hygiene` inspects URL query strings.** A credential riding in a URL
  (`macos_bootstrap_package: "https://…/bootstrap?token=<literal>"`) was
  invisible because only named credential fields were checked. `$VAR` and
  `op://` stay silent.
- **`flint dry-run --refresh-snapshot`** re-captures `.fleet-snapshot.json`
  before linting, so "hash is not uploaded" means *now* rather than at capture
  time; `[fleet] refresh_snapshot = true` opts a repo in for every run.
- **`--assume-uploaded`** treats every package as already on the server — for
  mid-upload work when refreshing is not possible.
- **`flint fleet doctor`** diagnoses the Fleet connection; config errors are
  reported loudly instead of silently falling back to defaults.
- **`flint paths --unwired --oneline` and `--prompt`**: one tab-separated record
  per orphan (a `grep` target), or a ready-to-run instruction per artifact for an
  agent.
- Fixes: an absolute path no longer falls out of `[files] include` scope, and
  `flint check <one file>` no longer panics when that file is filtered out.

## v0.2.1 (2026-08-11)

- **Schema tracks Fleet v4.90.0** (`controls.name_template`,
  `*_settings.assets`); a fleet file's own `mdm:` block is accepted.
- **Every key Fleet resolves as a path is a file reference.** `path-exists` and
  the orphan scan shared no list and neither matched Fleet, so live ADE
  enrollment profiles under `apple_setup_assistant` were reported as orphans
  while a typo in the same key linted clean.
- **`--fix` promises match what it applies**: six codes that advertised fixes
  the applier never emitted are no longer marked fixable, and `check` now says
  when a finding is auto-fixable.
- osquery table matrix synced to 5.23.1; the docs index leads with the
  check → dry-run → fix loop.

## v0.2.0 (2026-08-09)

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
