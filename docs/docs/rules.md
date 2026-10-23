---
icon: lucide/list-checks
---

# Lint rules

Every rule flint can report is listed on [All rules](rules-list.md), generated
from `flint list-rules` when the docs are built. Locally: `flint list-rules`
(add `--format json` for tooling).

## Configure rules

In `.fleetlint.toml`:

```toml
[rules]
disabled = ["orphaned-file"]       # never report these
warn = ["unregistered-script"]     # report errors from these as warnings
```

## Inline suppression

Suppress rules on one line with a comment at the end of it, or on the line
directly above it:

```yaml
queries:  # flint: ignore [deprecated-keys]
```

List several codes separated by commas, or leave the brackets out to suppress
every rule on that line:

```yaml
queries:  # flint: ignore [deprecated-keys, yaml-trailing-whitespace]
queries:  # flint: ignore
```

## Deprecations

Renamed keys, files and directories warn from the Fleet release that
introduced the new name; see [Migration](migration.md) for the list and
`flint migrate` for a repo-wide report.
