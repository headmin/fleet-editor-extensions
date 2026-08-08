# Tier 2 harness

Runs the **real `fleetctl gitops --dry-run`** against a mocked datastore, so
flint's severity decisions can be checked against the actual command.

No MySQL, no Redis, no Docker, no local Fleet checkout. ~1s.

## Why this is a copy-in and not a normal test

The harness must compile *inside* Fleet's own package: the mock setup it needs,
`setupEmptyGitOpsMocks`, is defined in a `_test.go` and is therefore
unimportable from anywhere else, however you depend on Fleet.

So `run.sh` takes the pinned Fleet module out of the **module cache**, makes a
writable copy, drops `flint_tier2_harness_test.go` in, and runs `go test`.

Deliberately *not* a `git clone` of fleetdm/fleet: the module-cache copy is the
exact source `go.sum` verifies and the exact source the Tier 1 oracle links, so
both tiers are guaranteed to audit the same Fleet. A clone could drift from the
pin silently.

The Fleet version is read from `../gitops-oracle/go.mod` — one source of truth
for both tiers.

## What it covers that Tier 1 cannot

Tier 1 (`tools/gitops-oracle`) calls `spec.GitOpsFromFile`, the parsing half.
Everything `fleetctl` does *after* parsing lives in the command layer and is
invisible to it:

- `VerifyLabelScopes` (include/exclude overlap) — run from the package-private
  `getLabelUsage`
- label existence resolved against the server
- premium gating per key
- fleet ordering, `no-team`/`unassigned` rules, secret handling

## Usage

```bash
./run.sh                                    # severity contract tests
FLINT_TIER2_REPO=/path/to/repo ./run.sh     # + dry-run a real repo
./run.sh -v                                 # verbose go test output
```

Export whatever `$VAR`s your configs interpolate (e.g. `FLEET_URL`) exactly as
`fleetctl` requires them — the harness does not default them, because silently
substituting a value would hide a real class of failure.

## The contracts it pins

Each case is a claim flint's rules make about Fleet. A rule reporting an ERROR
asserts "the apply fails"; a WARNING asserts "the apply succeeds, but". Getting
that backwards blocks commits on valid config — four such rules shipped before
this harness existed.

| case | expected | flint rule |
|---|---|---|
| zero-match glob | succeeds, advises | `broken-reference` = warning |
| `path:` to a missing file | **fails** | control — proves the harness detects real errors |
| policy label include∩exclude | **fails** | `label-targeting` = error |
| `labels_include_any: []` beside a real list | succeeds | presence measured by value |

The control case is load-bearing: a harness that never fails cannot distinguish
"Fleet accepts this" from "this harness is broken".

## Known labels

`newHarness(t, "Eng", "QA")` declares which labels the mocked server knows,
via `GetLabelSpecsFunc`. It must be set AFTER `setupEmptyGitOpsMocks`, which
stubs it to nil — otherwise any fixture referencing a label fails for the wrong
reason and masks what the case was testing.

This is the harness's main fidelity seam: label existence is real server state.
See the repo-level plan for consuming a label snapshot instead of a hardcoded
list.
