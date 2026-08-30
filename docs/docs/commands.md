---
icon: lucide/terminal
---

# Commands

Beyond linting, flint generates GitOps YAML from real artifacts, repairs and
wires `path:` references, and scaffolds new files. Run `flint <command> --help`
for full flags, or `flint help-ai --sop <lint|paths|software|history>` for
step-by-step workflows.

## The local dry-run

```bash
flint dry-run                     # lint the whole repo; verdict: would gitops pass?
flint dry-run --strict            # also gate on advisory warnings (zero-tolerance)
flint dry-run --json              # machine-readable verdict (for CI)
flint dry-run --exclude '**/tools-scripts/fleet-templates/**'
flint dry-run --against main       # lint the MERGE of HEAD and main, before merging
flint dry-run --oracle ./gitops-oracle   # dev/CI: audit blocking findings against Fleet's parser
```

`flint dry-run` is the **local, server-free equivalent of `fleetctl gitops
--dry-run`** — no Fleet server, no `fleetctl`. It runs the whole-repo lint and
prints a single verdict:

```
✓ Local dry-run: PASS — 101 file(s), 0 blocking (7 advisory warning(s))
# or
✗ Local dry-run: FAIL — 2 issue(s) would block `fleetctl gitops`:
  …/corp-fonts.yml:2  [software-source] software package has hash_sha256 but no url installer source
```

Errors are **blocking** (exit code 2 — drop it straight into CI); warnings are
advisory unless `--strict`. Run it before every push (see *Wire it in* below).

**`--against <REF>`** lints the tree git *would* produce by merging `REF` into
`HEAD` — without producing it. The incident it exists for: one branch deleted
profiles another had begun referencing. Each tree was valid alone and git saw
no conflict (no file was touched twice), so the defect existed only in the
combination and surfaced in CI, after the merge. A textual conflict is reported
and blocks (exit 2): flint cannot lint a tree that does not exist yet.

**`--oracle <PATH>`** (dev/CI only — `gitops-oracle` is not shipped) runs the
same tree through Fleet's own parser and reports two lists: findings flint
blocks on that Fleet accepts — each marked *expected* (a check Fleet enforces
server-side, invisible to the offline parser) or *REVIEW* (a possible false
positive) — and complaints Fleet raises that flint is silent on. Advisory: it
never changes the verdict or exit code.

A clean dry-run means flint found nothing, which is weaker than `fleetctl`
accepting the apply: it does not model `${VAR}` expansion against the real
environment, whether a bootstrap URL is live, or anything Fleet checks only at
apply time.

## Linting (detail)

`flint dry-run` is a whole-repo wrapper over `flint check`. Use `check` directly
for fixes, single files, and full diagnostics:

```bash
flint check .                     # full diagnostics for a repo (or a single file)
flint check . --fix               # auto-apply Safe fixes (renames, typos)
flint check . --fix --unsafe-fixes
flint check . --format json       # structured output for CI
flint check . --exclude '**/tools-scripts/fleet-templates/**'   # skip a folder
flint check wifi.mobileconfig     # scan one profile (or DDM .json) directly
flint list-rules                  # show enforced rules
```

`flint check <profile>` runs the profile-level rules (`profile-well-formed`,
`payload-uuid-format`) on a single `.mobileconfig` or DDM declaration without a
fleet file — for authoring in an editor. Wiring and reference rules still need
the directory lint. On a directory, the summary leads with a verdict and, when
the same finding text recurs in five or more files, a repeated-findings index —
so a 139-line wall reads as a few problems repeated rather than many problems.

Severity follows the coupling flint can see: a finding inside a fragment no
fleet wires (a broken `path:`, an empty target) is a **warning**; the same
finding inside a wired fragment is an **error**, because there it will fail
the apply.

See [Getting started](getting-started.md) and [Rules](rules.md).

### Wire it in

Make the local dry-run unavoidable so failures never reach the pipeline:

```bash
flint hooks install --pre-push    # block `git push` if the local dry-run fails
```
```bash
# CI — run BEFORE the server dry-run (e.g. in .github/.../gitops.sh):
flint dry-run . || exit 1         # local gate, no server needed
fleetctl gitops --dry-run         # server dry-run only if the local one passes
```

### Excluding files/folders

Skip template scaffolds or vendored YAML two ways:

- **CLI:** `--exclude '<glob>'` (repeatable), e.g.
  `flint check . --exclude '**/tools-scripts/fleet-templates/**'`.
- **`.fleetlint.toml`** (persistent, applies to every run):

  ```toml
  [files]
  exclude = ["**/tools-scripts/fleet-templates/**"]
  ```

CLI `--exclude` globs are merged with `files.exclude`. Excludes apply to both
the per-file rules and the cross-file graph pass.

### Catch dry-run failures locally (no server)

Many `fleetctl gitops --dry-run` rejections are statically detectable, so
`flint check .` catches them **before** the pipeline — no Fleet server or
`fleetctl` required. Beyond the per-file rules, a **cross-file graph pass** runs
on `flint check <dir>` (it needs the whole repo to resolve references):

- `label-reference` — a `labels_include_any/all` or `labels_exclude_any` value
  that names no label defined in the repo (and is not a Fleet built-in like
  `macOS` or `All Hosts`). Catches typos and forgotten definitions.
- `install-software-hash` — a policy `install_software.hash_sha256` with no
  matching software package anywhere in the repo.
- `install-software-team` — a fleet/team whose policy auto-installs a package
  that *that team* doesn't include in its `software` list (resolving
  `package_path`/`hash_sha256`/`fleet_maintained_app_slug`/`app_store_id`, and
  policies pulled in by a `paths:` glob). The install-on-fail can't run for that
  team, and gitops reports `package not found with hash`. Covers `fleets/` and
  `teams/`.
- `app-store-vpp` — `software.app_store_apps` declared but no
  `volume_purchasing_program` (VPP) configured in `org_settings`. App Store apps
  install via Apple's VPP; without it `fleetctl gitops` can't apply them. (The
  VPP token / `location` match / app availability are server-side — only the
  missing-config precondition is checked.)
- `install-software-id` — a policy whose query checks a different `package_id`
  than the package its `install_software` installs (compared against the
  identifier in the referenced software file's `# <id> (...)` header comment).
  Such a policy can install the package yet never pass, so Fleet reinstalls
  until the 3-attempt cap then stops. Only `package_receipts.package_id` queries
  are compared (an `apps.bundle_identifier` query is a different namespace and is
  skipped).

## Troubleshooting: policy installs software but never passes / never runs

A valid config can still misbehave at runtime. The usual causes, none of which
are dry-run errors:

1. **`package_id` mismatch** — the policy query checks one id but the package
   registers another, so the policy never goes green after install. Caught by
   `install-software-id`. Fix the query to match the id the package actually
   installs (`pkgutil --pkg-info <id>`).
2. **Fleet's 3-attempt cap** — policy-automation installs are tried up to 3
   times; the count resets only when the host passes. After a mismatch (or an
   earlier failure) exhausts the attempts, Fleet stops trying. Re-applying
   gitops does **not** reset it — deselect/reselect *Install software* in
   **Manage automations** to clear the count and retrigger.
3. **Package not uploaded / not in the team** — the install action needs a
   package with the referenced hash uploaded to the target server and present in
   the team's `software` list. Caught by `install-software-team` /
   `software-source`. (Install-on-policy-failure is Fleet Premium; installs time
   out after 1 hour; the software's label targeting must include the host.)
- `software-url` — a `software.packages[].url` (or a standalone software file's
  `url:`) that is malformed, plus the unfilled `REPLACE-ME` placeholder that
  `flint gen software --standalone` emits.
- `software-source` — a software package with a `hash_sha256` but no `url`.
  Fleet can install it only if a package with that hash is already cached on
  the server; otherwise gitops fails with `package not found with hash`. This
  is what happens when `flint gen software`'s minimal block (`- hash_sha256: …`)
  is used as a software file instead of `flint gen software --standalone`
  (which scaffolds a `url:`).

Both cross-file checks are **warnings**: GitOps legitimately allows referencing
a server-side label or a package already cached in Fleet by hash, so an
unresolved reference is a likely mistake, not a certain one. They run only on a
directory (not a single file) and are configurable by code in `.fleetlint.toml`.

## Path references

flint understands the repo's `path:` / `paths:` dependency graph — both
directions.

### Fix broken references

After a folder reorg, references to moved files break silently. `flint paths`
finds them and, when a file's new location is unambiguous, fixes them.

```bash
flint paths .                     # report broken path: refs (before → after)
flint paths . --fix               # rewrite every unambiguous reference in place
```

When no unique match exists on disk, the report says what git knows about the
missing target — `renamed to <new> in <sha> "<subject>"` or `deleted in <sha>
"<subject>"`. A recorded rename is unambiguous, so `--fix` rewrites it too
(following a chain of up to five renames); a deleted target gets no fix — drop
the reference or restore the file.

It also catches a reference to an **empty file** (`path-empty`): a `path:`/
`package_path:`/`run_script.path`/`install_script.path`/… that resolves but has
no usable content — blank, whitespace-only, or (for YAML) comment-only (e.g. a
software file with just a `# … version …` header and no `hash_sha256:`, or an
empty `.sh`). It resolves on disk but `fleetctl gitops` rejects it. Covers every
path-bearing key (software, policies, scripts, profiles); the software file
itself is also flagged when linted directly (`software-source`).

This also catches **case-only mismatches** (`path-case`): a reference like
`../lib/Foo.yml` when the file on disk is `lib/foo.yml` resolves fine on
case-insensitive macOS but **fails on case-sensitive CI** (`fleetctl gitops` on
Linux) — the classic "passes locally, breaks the pipeline" trap. flint compares
each path component against the real on-disk name and suggests the exact casing
as a Safe fix. Applies to `path:` and `install_software.package_path`.

In the editor, broken references get a quick-fix lightbulb (one option per
candidate) plus a **“Fix all N references to <file>”** action that repairs every
referrer across the workspace.

### Wire unwired artifacts

The inverse: files that exist on disk but no fleet/team config references.

```bash
flint paths . --unwired                 # report orphans, grouped + copy-paste blocks
flint paths . --unwired --interactive   # walk each group, wire into chosen fleets
```

Interactive wiring asks per fleet (`[y]es / [a]ll-remaining / [n]o / [s]kip /
[q]uit`), then per group offers a **glob** (one rule for the directory) or
**per-file** entries that can carry `labels_include_any` / `labels_exclude_any`
(combinable: broad include + targeted exclude).

| Flag | Effect |
|---|---|
| `--label-stubs[=blank\|comment]` | on blank label answers, emit the empty key (`blank`, default) or a commented stub (`comment`) |
| `--only <glob>` | limit target fleets, e.g. `--only 'fleets/team-*.yml'` |

Each artifact is routed to the right section using current Fleet keys
(`controls.apple_settings.configuration_profiles`, `controls.scripts`,
`controls.setup_experience.apple_setup_assistant`, `software.packages`, …), with
paths written relative to where your fleet files actually live.

An artifact kept on purpose — an archived profile, a template — is *declared*
rather than excluded:

```toml
[orphans]
allow = ["profiles/archive/**"]
```

A match silences `orphaned-file` for that artifact and nothing else; `[files]
exclude` would take the file out of scope for every rule, including the ones
that check its contents.

## Generate YAML (`flint gen`)

One verb for flint's generator face: `flint gen <kind> [--from <source>]`.
The `--from` source's extension picks the generator; omitting `--from` emits a
blank template where one exists.

### Software stanzas

Read an installer and emit the `software.packages` block — no hand-transcription
of identifiers, versions, or hashes.

```bash
flint gen software --from app.pkg                 # comment header + hash_sha256
flint gen software --from app.pkg --full          # full stanza: url placeholder, self_service, stubs
flint gen software --from app.pkg --full --setup-experience   # add setup_experience: true (OOBE)
flint gen software --from app.pkg --wire          # generate + interactively insert into a fleet
flint gen software --from app.pkg --standalone    # write a standalone <name>.package.yml next to the .pkg
flint gen software --from app.pkg --standalone --full -o ./software   # full scaffolding, random name in ./software
flint gen software --from app.pkg --standalone -o app.yml             # exact filename (refuses to overwrite)
flint gen software --from installer.deb           # other formats (see below)
flint gen software --from ./packages/             # batch a directory of .pkg files
```

`--standalone` writes a complete **standalone** software file — the top-level
mapping form a fleet/team file references via `software.packages: [- path: …]`
— rather than the inline list-item block. The name is randomized
(`<name>.package.yml`, name derived from the installer) so running it repeatedly never overwrites an earlier
file; pass `-o <dir>` to choose the directory, or `-o <name>.yml` for an exact
filename. `hash_sha256` (the value `flint gen software` computes) leads the
file; `url` is a fill-in placeholder. Honors `--full` and `--setup-experience`.

Non-`.pkg` sources cover the other Fleet custom-package formats, extracting
metadata where the format and tooling allow (the SHA-256 is always computed):

| Format | Source |
|---|---|
| `.pkg` | `xar` (identifier + version) |
| `.deb` | `ar` + `tar` (Package / Version) |
| `.ipa` | `unzip` + `plutil` (CFBundleIdentifier / CFBundleShortVersionString) |
| `.msi` | `msiinfo` (msitools), if installed |
| `.rpm` | `rpm`, if installed |
| `.exe`, `.tar.gz` | hash + placeholders |

> `.dmg` is not a Fleet custom-package format — repackage the `.app` inside as a
> `.pkg` first.

### Keep a package current

On a new release of an already-referenced package:

```bash
flint gen software --from new.pkg --update software/app.yml
```

Matches the stanza by identifier and refreshes `hash_sha256`, version, and the
`url` filename — leaving comments, labels, and `self_service` intact.

### Install-state policies & default scripts

```bash
flint gen policy --from app.pkg                  # "installed & up to date" via package_receipts
flint gen policy --from app.pkg --apps           # via the apps table (bundle_identifier)
flint gen policy --from app.pkg --enforce        # + install_software automation (auto-install on fail)
flint gen policy --from app.pkg --outdated-only  # fail only when an outdated copy is installed
flint gen policy --from app.pkg --file '/Applications/YourApp.app/Contents/Info.plist'  # file-exists proxy
flint gen scripts --from app.pkg -o ./scripts    # Fleet's default install.sh + uninstall.sh
```

Three verification strategies:

- **default** — `package_receipts` by `package_id` (+ semantic version).
- **`--apps`** — the `apps` table by `bundle_identifier` / `bundle_short_version`
  (for packages that install a `.app`).
- **`--file <PATH>`** — the `file` table: `SELECT 1 FROM file WHERE path = '<PATH>'`.
  An existence proxy (no version) for packages that don't register a clean
  receipt or app bundle — fonts, configs, dropped files. Combine with `--enforce`
  to auto-install on a missing file.

The version query uses `EXISTS (…) AND NOT EXISTS (… version_compare(…) < target)`
on osquery's semantic `version_compare`, **not** a single `… >= target`: a
package can have multiple receipts, and the single-comparison form passes if any
copy is current even when an older one lingers. The `EXISTS/NOT EXISTS` form
passes only when the package is installed **and** no copy is older than the
target — fails on missing or outdated.

- `--outdated-only` drops the `EXISTS` gate: the policy passes when the package
  is **absent or current** and fails only on an outdated copy — patch existing
  installs without forcing a fresh install on hosts that don't have it.

By default it is an **audit** policy — it reports drift but doesn't remediate.
Add `--enforce` to append an `install_software` automation that references the
package by the `hash_sha256` `flint gen software` computed, so Fleet
auto-installs it on hosts that fail the policy. The package must also be listed
under `software` in the same team (by that hash). The scripts are Fleet's
verbatim defaults (`$PACKAGE_ID` is substituted by the Fleet server at upload).

### Configuration profiles

```bash
flint gen profile --from wifi.mobileconfig   # → configuration_profiles entry
flint gen profile --from passcode.json       # DDM declaration (Type + Identifier)
flint gen profile --from firewall.xml        # Windows CSP (LocURI context)
flint gen profile --from enroll.dep.json     # → setup_experience.apple_setup_assistant
flint gen profile --from wifi.mobileconfig --full   # add labels_* stubs
flint gen profile --from wifi.mobileconfig --wire   # insert into a fleet (path per-fleet-relative)
```

The header includes the display name, identifier, scope, and the nested
`PayloadType` (the meaningful one, not the `Configuration` wrapper).

!!! note "Optional: Apple-schema validation via contour"

    flint checks the *Fleet* side of a profile — is it referenced, is it wired
    into a fleet, is its PayloadUUID unique — and never inspects the payload
    itself. If [`contour`](https://github.com/macadmins/contour) is installed,
    `gen profile` additionally validates the profile against Apple's schema
    and prints anything it finds to **stderr**, so stdout stays
    copy-pasteable and `flint gen profile … > entry.yml` is unaffected.

    Entirely optional: with contour absent the output is byte-identical and
    nothing is printed. Absence of findings therefore means "not checked",
    not "no problems".

Mitigate a `duplicate-payload-uuid` finding by minting a fresh UUID:

```bash
flint gen profile --from dup.mobileconfig --regen-uuid
```

### Queries & blank templates

```bash
flint gen query --from check.sql             # query stanza; platform inferred from osquery tables
flint gen policy --from check.sql            # policy stanza around that query
flint gen profile                            # starter .mobileconfig (fresh PayloadUUID)
flint gen fleet|policy|query|label           # blank templates
```

`gen query` infers `platform:` from the intersection of the platforms of the
osquery tables the SQL references; unknown tables leave a commented placeholder.

### Migrating from v0.1.x

The legacy generator commands were **removed in v0.3.0**. An old spelling now
fails as an unrecognized subcommand; this table is the migration guide:

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

## Rule history (`flint history`)

Replays today's rules against past commits, so rule priority rests on measured
recurrence rather than judgement. Each first-parent commit is reconstructed with
`git archive` into a scratch directory — the working copy is never touched —
and linted.

```bash
flint history --max 400                  # replay; red windows per rule (default 200 commits)
flint history --since v0.2.2             # replay REF (exclusive) .. HEAD
flint history --suggest-patterns         # mine remediation commits for [[patterns]] guardrails
flint history --oracle ./gitops-oracle   # diff blocking verdicts against Fleet's own parser
flint history --gate .flint/baseline.json   # CI: exit 2 when rule quality regresses
flint history --json                     # every mode, machine-readable
```

Findings are grouped into **red windows** — runs of commits in which a code
fired, and the commit that closed each one:

```
path-exists  ×3 closed windows, 41 commit(s)
    a1b2c3d Move profiles into platforms/  →  fixed in e4f5a6b Repoint the moved profiles
    …
```

Only **closed** windows score. A finding still present at HEAD describes current
state, not a repeated mistake; a code with two or more closed windows is a
repeat failure, and that is the ranking that matters. `software-source` is not
replayed and says so: a snapshot-derived finding depends on the Fleet server's
state at that commit, which is gone.

- **`--suggest-patterns`** mines commits that look like remediation and
  proposes `[[patterns]]` guardrails for conventions repaired at least
  `--min-occurrences` times (default 2 — one repair is an anecdote). The output
  is heuristic, emitted commented out, and never written to your config.
- **`--oracle <PATH>`** (dev/CI only) puts each tree through
  `spec.GitOpsFromFile` — the function `fleetctl gitops` itself calls — and
  diffs blocking claims both ways: rules that say an apply will fail where Fleet
  accepts (marked *expected* where Fleet enforces the check server-side, else
  *REVIEW*), and Fleet complaints flint is silent on.
- **`--gate <PATH>`** compares the run to a stored scorecard and exits 2 on
  regression: a rule newly blocking where Fleet accepts, a Fleet complaint flint
  newly misses, or a rule that no longer catches what it used to. Only new
  *keys* gate — occurrence counts move with the range replayed, so they are
  reported and never failed on. The file is written and the run passes when it
  does not exist yet; `--update-baseline` overwrites it once a change is
  understood.
- **`--scope-as-committed`** replays each tree under its *own* committed
  `.fleetlint.toml` instead of today's, answering "what would flint have said
  at the time" rather than "today's rules against yesterday's trees".

## Fleet Maintained Apps (`flint fma`)

Slug lookup for `fleet_maintained_apps:` entries and patch policies — the
same registry that powers editor completions and the `fma-slug` lint rule.

```bash
flint fma search slack            # ready-to-paste slugs (slack/darwin, slack/windows)
flint fma show raycast            # platforms, latest version, installer URL
flint fma latest --days 7         # recent version bumps from fmalibrary.com
flint fma refresh                 # update the local registry cache from the feed
```

`refresh` writes `~/.cache/flint/fma-cache.toml`; lint, completions, and
search all pick it up immediately. Everything accepts `--json`.

## Fleet instance views (`flint fleet`)

Read-only — these commands can never modify your instance. Connection comes
from `.fleetlint.toml` `[fleet]` (or `~/.config/flint/config.toml`), with
`FLEET_URL`/`FLEET_API_TOKEN` env or `./.env` fallback; `op://` secret
references resolve via the 1Password CLI.

```bash
flint fleet status                        # server version — connection sanity check
flint fleet software --team 5 --available # installable titles on the instance
flint fleet fma                           # FMA catalog YOUR Fleet version offers
flint fleet labels                        # labels defined on the instance
flint fleet teams                         # teams (fleets) with host counts
```

## Agents & editors

```bash
flint setup-agent                 # install Claude Code skills
flint help-ai                     # command reference for agents
flint help-ai --sop paths         # broken-path + unwired workflows
flint help-ai --sop software      # artifact-generation workflows
flint help-ai --sop history       # replay / suggest-patterns / oracle / gate workflows
flint lsp                         # language server (run by editor extensions)
```

See [Editors](editors.md) for editor setup.
