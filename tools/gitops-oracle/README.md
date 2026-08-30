# gitops-oracle

Reports what **Fleet's own parser** thinks of a set of GitOps YAML files, so
flint's rules can be diffed against ground truth instead of against a reading
of the docs.

Not shipped. Dev/CI only — flint's released binary stays pure Rust.

## Why

flint reimplements Fleet's GitOps validation in Rust, and reimplementations
drift. Drift in one direction is expensive: a rule claiming *"Fleet rejects
this at apply time"* when Fleet does not sends users to edit working config.
Five such rules shipped before this tool existed:

| rule | claimed | Fleet actually |
|---|---|---|
| `label-targeting` | one `labels_*` key per item, everywhere | per-context contracts; profiles allow include+exclude |
| `categories` | only the seed set is valid | custom names are legal |
| `fma-slug` | 232-app registry | 1141 apps |
| `broken-reference` (glob-zero) | blocks the apply | logs `[!]` and continues |
| org logo keys | — | two renames flint didn't know, and it *advertised* the deprecated names |

Each was found by hand, after the fact, usually because a repo that applies
cleanly lit up with hundreds of findings. This tool finds them by construction.

## What it covers

It calls `spec.GitOpsFromFile` — the exact function `fleetctl gitops` calls —
so it covers the offline half of validation: structure, unknown keys,
deprecated renames, `$VAR` expansion, `path`/`paths` resolution, glob
expansion, duplicate basenames, extension allowlists, policy→script/software
cross-references, and software label rules.

Policy label scoping is **not** in that call tree — `fleetctl` runs it from the
command layer (`cmd/fleetctl/fleetctl/gitops.go:1106`, inside the
package-private `getLabelUsage`). `PolicySpec.VerifyLabelScopes` is exported
and pure, so the oracle calls it directly to close that gap.

**Cannot cover** anything needing live server state: whether a label exists,
team IDs, VPP/ABM tokens, software title IDs, `FLEET_SECRET_*` presence. Those
need a Tier 2 harness (real command against a mocked Fleet API — see
`RunServerWithMockedDS`, which needs no MySQL/Redis/Docker if you pass
`Pool: redistest.NopRedis()`).

## Entry files vs fragments

Fleet's parser only judges a **fleet file** (`name:`) or the **global config**
(`org_settings:`). Profiles, standalone policy/query lists, and software specs
are validated through the parent that references them, never alone — in the
reference repo that's 187 of 214 YAML files. Feeding one to the parser yields
an unmarshal error that looks like a finding but only means "wrong entry
point", so the oracle classifies files by top-level shape first and marks
fragments `not_entry_file`. The comparison skips them.

This is the tool's main fidelity limit: **it says nothing about fragments.**
flint's per-file rules on those files are unaudited by this oracle.

## Scope is a shared contract

The oracle reads the repo's `.fleetlint.toml` `[files]` section and applies the
same rules flint does (`FleetLintConfig::is_out_of_scope_file`): `exclude`
wins, and a non-empty `include` is *authoritative* — a path matching none of
its globs is out of scope, not merely un-narrowed. Discovery walks upward from
the target, like flint's own config lookup.

This is not a convenience. A differential audit is only meaningful if both
tools are asked about the same files; otherwise every scoping difference
surfaces as a false gap. Before this existed, the first run reported 10
"missing flint rules" that were entirely `tools-scripts/fleet-templates` — a
directory flint was configured to ignore. After it, the same repo reports
**zero disagreements**.

`-no-scope` inspects everything regardless, for when you want to see what the
config is hiding.

Two deliberate differences from flint, both documented in `scope.go`:

- flint's "default to YAML-only when no `include` is set" is not copied — the
  oracle already walks only `*.yml`/`*.yaml`, so repeating it would be a no-op
  that invites drift.
- A bare directory prefix (`platforms/_retired`) is treated as covering
  everything beneath it. flint's globset wants the explicit `/**`; accepting
  both stops a punctuation difference from reading as a real gap.

`scope_test.go` pins the semantics, including that an anchored `santa/**`
exclude must NOT reach a nested `platforms/macos/site/santa/` — the trap that
would silence live config.

## Usage

```bash
go build -o gitops-oracle .

# one repo, JSON to stdout
./gitops-oracle -repo /path/to/gitops-repo > oracle.json

# diff against flint
flint check /path/to/gitops-repo --format json > flint.json
python3 compare.py oracle.json flint.json
```

Flags: `-premium` (default true — most repos use premium keys), `-repo`,
`-base`, `-pretty`.

Export any `$VAR` your configs interpolate (e.g. `FLEET_URL`) or the parser
reports them as unset, exactly as `fleetctl` would.

`compare.py` prints two lists:

- **ORACLE-ONLY** — Fleet complains, flint silent → a missing flint rule.
- **FLINT-ONLY** — flint complains, Fleet silent → *candidate* false positive.

"Candidate" is load-bearing. flint deliberately reports things the parser
structurally cannot see: cross-file rules, workspace `[[patterns]]`, and
hygiene rules. Those are listed in `FLINT_ONLY_BY_DESIGN` in `compare.py`; add
an entry only with a reason.

Matching is by *quoted subject*, not whole message — the tools phrase and quote
findings differently (Fleet `"double"`, flint `'single'`), and comparing whole
strings manufactures disagreements.

## Pinning Fleet

`go.mod` pins a published Fleet version:

```
require github.com/fleetdm/fleet/v4 v4.90.0
```

No `replace`, no local checkout, no machine-specific path — `go build` works
anywhere. Fleet's own `go.mod` carries zero replace directives, which is what
makes it consumable as an ordinary dependency.

### Pin to the SERVER version, not to latest

The pinned version should match the Fleet your **server** runs, not `main`.
The oracle answers *"does flint match the Fleet that will actually reject this
config?"* — linking `main` validates against behavior your deployment does not
have yet. That is not hypothetical: during one session the local checkout
advanced 12 commits mid-work and picked up
`settings.webhook_settings.host_activities_webhook`, a key that does not exist
for a 4.90.0 server. Findings derived from it would be true for future Fleet
and wrong for production.

Check what you run:

```bash
curl -s "$FLEET_URL/api/v1/fleet/version" -H "Authorization: Bearer $TOKEN" | jq -r .version
```

Bump the `require` line when the server is upgraded. That bump is the whole
maintenance burden.

### Upgrade pre-flight, for free

Because the version is one line, build twice and diff:

```bash
go build -o oracle-current .                                  # server version
go mod edit -require=github.com/fleetdm/fleet/v4@v4.90.0
GOSUMDB=off go mod tidy && go build -o oracle-next .
./oracle-current -repo REPO > now.json
./oracle-next    -repo REPO > next.json
```

Anything in `next.json` but not `now.json` is what the Fleet upgrade will
start enforcing — answerable *before* upgrading.

### GOSUMDB note

`sum.golang.org` returns 404 for Fleet's `go.mod`, so the FIRST resolution of a
new version needs `GOSUMDB=off`:

```bash
GOSUMDB=off go mod tidy
```

Only that one step. `go.sum` is committed, so ordinary `go build` verifies
against it locally and never contacts the checksum database. You only need the
flag again when you bump the pinned version.

## Interpreting a run

A clean result is `FLINT-ONLY: 0` plus an ORACLE-ONLY list containing only
files your `.fleetlint.toml` deliberately excludes. Anything else is work:
oracle-only findings are rules to add, flint-only findings are claims to
re-verify against source before trusting.
