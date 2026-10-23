---
icon: lucide/flask-conical
---

# Rules by example

Each example below is a small GitOps repo. A code block's title is the file's
path in that repo, and a trailing `# error: [rule]`, `# warning: [rule]` or
`# info: [rule]` comment marks what `flint check` reports on that line; alone
on a line, it marks the next line. `# file-warning: [rule]` marks a finding
about the whole file. Code blocks without a title are not part of the repo.
Every repo also has a valid `default.yml` unless the example shows one.

These examples are flint's tests: the test suite builds each repo, runs
`flint check`, and fails if flint reports anything other than what is marked
here. So this page always matches what flint does.

## structural-validation

An unknown key is an error, and a close match gets a "did you mean":

```yaml title="fleets/workstations.yml"
name: Workstations
polices: []  # error: [structural-validation]
```

## yaml-duplicate-key

Fleet keeps the last of two equal keys, so the first one silently does nothing:

```yaml title="fleets/workstations.yml"
name: Workstations
policies: []
policies: []  # error: [yaml-duplicate-key]
```

## path-exists

A `path:` must name a file that exists, relative to the file it's in:

```yaml title="fleets/workstations.yml"
name: Workstations
controls:
  scripts:
    - path: ../lib/scripts/missing.sh  # error: [path-exists]
```

## orphaned-file

A script, profile or payload that no fleet references is never applied:

```yaml title="fleets/workstations.yml"
name: Workstations
```

```sh title="lib/scripts/cleanup.sh"
#!/bin/sh
# file-warning: [orphaned-file]
echo cleanup
```

## duplicate-names

Two policies with the same name in one file collide in Fleet:

```yaml title="fleets/workstations.yml"
name: Workstations
policies:
  - name: Disk encryption
    query: SELECT 1 FROM disk_encryption WHERE encrypted = 1;
  - name: Disk encryption  # error: [duplicate-names]
    query: SELECT 1 FROM disk_encryption WHERE encrypted = 1;
```

## interval-validation and query-syntax

Report findings point at the report's `name:` line:

```yaml title="fleets/workstations.yml"
name: Workstations
reports:
  # warning: [interval-validation]
  # info: [query-syntax]
  - name: Running processes
    query: SELECT * FROM processes;
    interval: 10
```

The `SELECT *` info is off with:

```toml
[thresholds]
warn_select_star = false
```

## env-var-resolvable

fleetctl expands `${VAR}` everywhere in the file, comments included. A name
that is not set anywhere fails the apply:

```yaml title="fleets/workstations.yml"
name: Workstations
policies:
  - name: Uses a variable
    query: SELECT 1 WHERE '${SITE_CODE}' != '';  # warning: [env-var-resolvable]
```

Declare names the repo supplies to fleetctl in `fleetlint.toml`:

```toml
[fleet.env]
SITE_CODE = "from CI"
```

## fleet-variable

`$FLEET_VAR_` names must be ones Fleet defines, and allowed in that file type:

```yaml title="fleets/workstations.yml"
name: Workstations
controls:
  scripts:
    - path: ../lib/scripts/hello.sh
```

```sh title="lib/scripts/hello.sh"
#!/bin/sh
echo "$FLEET_VAR_HOST_END_USER_EMAIL_IDPP"  # error: [fleet-variable]
```

## reserved-fleet-name

Only `unassigned.yml` may use the reserved name `Unassigned`:

```yaml title="fleets/workstations.yml"
name: Unassigned  # error: [reserved-fleet-name]
```

## Suppressing a finding

A `# flint: ignore [rule]` comment on the line, or alone on the line above,
suppresses it. One that suppresses nothing is reported, so stale comments
don't pile up:

```yaml title="fleets/workstations.yml"
name: Workstations
polices: []  # flint: ignore [structural-validation]
# warning: [unused-ignore-comment]
# flint: ignore [duplicate-names]
policies: []
```
