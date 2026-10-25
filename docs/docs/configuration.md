---
icon: lucide/settings
---

# Configuration

Create `fleetlint.toml` in your repo root, or run `flint init` to generate one
interactively. Both spellings are read — `fleetlint.toml` and the hidden
`.fleetlint.toml` — with the visible one winning in the same directory.

## Full example

```toml
[rules]
disabled = ["secret-hygiene"]        # Rules to skip entirely
warn = ["interval-validation"]       # Downgrade from error to warning

[thresholds]
min_interval = 60                    # Minimum query interval (seconds)
max_interval = 86400                 # Maximum query interval (24h)
max_query_length = 10000             # Maximum query length (characters)
warn_select_star = true              # Warn on SELECT *

[files]
# `include` unset means "everything not excluded" — the usual case.
# If you narrow scope, list DIRECTORIES, not extensions:
#   include = ["default.yml", "fleets/**", "labels/**", "platforms/**"]
# A non-empty include is authoritative and also scopes the cross-file rules
# (orphaned-file, duplicate-content, case-collision), which report on
# scripts and profiles — so an extension glob like "**/*.yml" would silently
# switch those off for every non-YAML file.
exclude = ["node_modules", "target", "dist"]

[schema]
allow_unknown_fields = false         # Allow keys not in schema

[deprecations]
fleet_version = "latest"             # Target Fleet version for deprecation checks
future_names = false                 # Opt-in to new naming (reports, settings, fleets)

[fleet]
url = ""                             # Fleet server URL (optional)
token = ""                           # API token (supports $ENV_VAR and op://)
fleetctl_path = "fleetctl"           # Path to fleetctl binary
gitops_validation = false            # Run fleetctl --dry-run on save (LSP only)
live_completions = false             # Fetch labels/queries from Fleet (LSP only)
```

## Sections

### `[rules]`

- **`disabled`** (or **`ignore`**) — rules to skip entirely
- **`warn`** — rules whose errors become warnings
- **`error`** — rules whose warnings and infos become errors

When a rule is in several lists, `ignore` wins over `warn`, and `warn` wins over `error`.

### `[[overrides]]`

Rule levels for some paths only. The globs are relative to the config file, like `[files]`. A matching override wins over `[rules]`, and a later override wins over an earlier one:

```toml
[[overrides]]
files = ["fleets/legacy/**"]
rules.warn = ["structural-validation"]

[[overrides]]
files = ["lib/scripts/vendor"]       # a bare name matches at any depth
rules.ignore = ["orphaned-file"]
```

An override can also turn back on a rule that `[rules]` ignores, for the files it matches.

On the command line, `flint check --error RULE`, `--warn RULE` and `--ignore RULE` win over both. Each flag can be repeated or take a comma-separated list.

### `extend`

```toml
extend = "../org-base/fleetlint.toml"   # relative to this file
```

The base config is loaded first, and this file is laid over it. Tables merge key by key; any other value, including a list, replaces the base's. A base can `extend` another base. Use this to share one base config across several GitOps repos.

### `[deprecations]`

- **`fleet_version`** — target version for deprecation phase calculation. Set to your Fleet server version (e.g., `"4.85.0"`) to get appropriate deprecation warnings.
- **`future_names`** — when `true`, promotes dormant deprecations to warnings even before `deprecated_in` version. Useful for early adoption of new naming.

### `[fleet]`

Optional Fleet server connection for LSP features:

- **`gitops_validation`** — runs `fleetctl gitops apply --dry-run` on save
- **`live_completions`** — fetches labels, fleets, and reports from Fleet server for autocomplete

Credentials support environment variables (`$FLEET_API_TOKEN`) and 1Password references (`op://vault/item/field`).

## Editor support

A JSON Schema for this file is published at
`https://flint.macadmin.me/schemas/fleetlint.json`, generated from flint's own
config types. Editors that use taplo (for example VS Code's Even Better TOML)
then complete keys and rule names and mark typos as you type. `flint init`
writes the line that points them at it:

```toml
#:schema https://flint.macadmin.me/schemas/fleetlint.json
```

## File patterns

Flint activates for YAML files matching Fleet GitOps layouts:

```
default.yml              fleets/*.yml           platforms/**/*.yml
labels/**/*.yml          teams/*.yml (legacy)   lib/**/*.yml (legacy)
```

Both v4.83 (`platforms/`) and legacy (`lib/`) layouts are supported.
