# Repo patterns

Every GitOps repo grows conventions of its own: naming schemes, folder
layouts, "this payload must be wired into every fleet." Fleet can't know
them — but they break just as expensively as schema errors, usually during
renames. `[[patterns]]` entries in `.fleetlint.toml` let you encode those
conventions declaratively, so flint enforces them without you writing any
code.

```toml
[[patterns]]
files = "fleets/*.yml"
assert = "name-matches-filename"
severity = "warn"
why = "fleet names drifted from filenames after renames (commit abc123)"
```

Three things every pattern needs:

- **`files`** — a glob (repo-root relative) selecting what the pattern
  applies to.
- **`assert`** — one of the nine assertion kinds below.
- **`why`** — required. flint refuses to load a pattern without one, and
  prints it with every finding. A convention nobody can justify is noise,
  and noise is how linters get disabled. Cite the commit or incident that
  taught you the rule.

`severity` is optional: `error`, `warn` (default), or `info`. Findings
carry the code `pattern/<assert>` and respect `[rules] disabled`,
`[rules] warn`, and inline `# fleet-lint: ignore` suppressions like any
built-in rule. Patterns run during directory lints (`flint check .`), the
pre-commit hook, and the editor's on-save workspace pass.

## Assertions

### `name-matches-filename`

The YAML `name:` key (or another key, via `key = "..."`) must equal the
file's stem. Catches the classic rename drift where `ABC-DELTA.yml` still
says `name: ABC - Old Name`.

```toml
[[patterns]]
files = "fleets/*.yml"
assert = "name-matches-filename"
why = "fleet renames kept missing the name: key"
```

### `filename`

Every matched file's name must match a regex. The usual use is enforcing
lowercase-only names so case-only renames can't break case-sensitive CI.

```toml
[[patterns]]
files = "platforms/**"
assert = "filename"
regex = "^[a-z0-9][a-z0-9.-]*$"
why = "case-only rename passed locally, failed CI (commit ccbd016)"
```

### `content-must-match` / `content-must-not-match`

The file's content must (or must not) match a regex. Useful for keeping
brand or environment literals out of shared payloads.

```toml
[[patterns]]
files = "platforms/macos/base/**/*.mobileconfig"
assert = "content-must-not-match"
regex = "(?i)(brand-a|brand-b)"
why = "a brand literal leaked into a shared baseline payload"
```

### `token-consistency`

A token taken from a path segment (0-based `segment` index of the
root-relative path) must appear in the filename. Enforces layouts where a
purpose or brand folder name must be carried by the files inside it.

```toml
[[patterns]]
files = "platforms/macos/L2/*/*.mobileconfig"
assert = "token-consistency"
segment = 3            # platforms/macos/L2/<PURPOSE>/...
why = "a CFG payload sat in the CCS folder for a month"
```

### `must-be-referenced`

Matched files must be referenced (via `path:` or a `paths:` glob) by config
files matching `by`. `quantifier = "any"` (default) flags files nothing
references; `"all"` requires every `by` file to reference them — fan-out
completeness for baseline payloads.

```toml
[[patterns]]
files = "platforms/macos/base/**"
assert = "must-be-referenced"
by = "fleets/*.yml"
quantifier = "all"
why = "a baseline profile missed one fleet during fan-out"
```

### `unique-content-within`

No two matched files may be byte-identical. Scoped duplicate detection —
byte-identical payloads are copy-paste divergence waiting to happen.

```toml
[[patterns]]
files = "platforms/macos/L2/**/*.mobileconfig"
assert = "unique-content-within"
why = "the same payload was pasted into two purposes, then diverged"
```

### `required-structure`

Every directory matching `files` must contain the listed entries. Catches
folder shells left behind by renames.

```toml
[[patterns]]
files = "platforms/macos/site/*"
assert = "required-structure"
entries = ["configuration-profiles"]
why = "module renames left empty folders that globs still pointed at"
```

### `forbid-file`

Matched files may simply not exist in the repo.

```toml
[[patterns]]
files = "**/.DS_Store"
assert = "forbid-file"
why = "Finder droppings keep sneaking into commits"
```

## How patterns relate to the built-in cross-file rules

flint's built-in workspace rules cover the defects every Fleet GitOps repo
shares — `broken-reference` (a `paths:` glob matching nothing),
`case-collision`, `orphaned-file`, `unregistered-script`,
`duplicate-content`, `duplicate-identifier`, and `label-reference`. Those
need no configuration. Patterns are for what's true only in *your* repo.
If you find yourself wanting the same pattern in every repo you touch,
that's a hint it should become a built-in — open an issue.

## Pre-commit scoping

Patterns (and all cross-file rules) run repo-wide, but `flint check
--staged` reports only findings that touch a git-staged file — directly or
through the *other* file involved (a broken reference's renamed target,
a duplicate's twin). Pre-existing findings elsewhere are summarized, not
blocking, so patterns are adoptable on a repo that isn't clean yet.
`flint hooks install` wires this up as a pre-commit hook.
