---
icon: lucide/list-checks
---

# Lint rules

Every rule flint can report is listed on [All rules](rules-list.md), generated
from `flint list-rules` when the docs are built. Locally: `flint list-rules`
(add `--format json` for tooling).
[Rules by example](rule-examples.md) shows the common ones on small repos;
those examples are also flint's tests.

## Configure rules

In `fleetlint.toml`:

```toml
[rules]
disabled = ["orphaned-file"]       # never report these (alias: ignore)
warn = ["unregistered-script"]     # report errors from these as warnings
error = ["env-var-resolvable"]     # report warnings from these as errors
```

Per-path levels go in `[[overrides]]`, and `flint check --error/--warn/--ignore RULE`
wins over both; see [Configuration](configuration.md).

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

A comment that suppresses nothing, or a code in it that suppresses nothing, is
reported as `unused-ignore-comment`, so a stale comment can't hide a future
finding. `flint check --fix` removes it. This check runs on whole-repo lints,
where every rule has had its turn. A code no rule emits is reported as
`ignore-comment-unknown-rule`.

To adopt flint in a repo that already has findings, accept them as a baseline:

```bash
flint check . --add-ignore
```

This adds `# flint: ignore [rule]` to every line with a finding, so only new
findings are reported from then on. A line is left alone when a comment there
would change the YAML's data (inside a multi-line query, for example), or when
it already has another comment. Findings about a whole file have no line; use
[`[[overrides]]`](configuration.md) for those.

## Deprecations

Renamed keys, files and directories warn from the Fleet release that
introduced the new name; see [Migration](migration.md) for the list and
`flint migrate` for a repo-wide report.
