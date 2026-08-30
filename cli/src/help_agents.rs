//! Generate machine-readable CLI reference for AI agents.
//!
//! Progressive discovery modes:
//! - **Index** (default): Agent guide + command index
//! - **Command**: Full detail for a single command by dotted path
//! - **Full**: Complete CLI reference (all commands, all flags)
//! - **SOP**: Step-by-step standard operating procedures
//! - **JSON**: Full CLI schema as JSON

use std::fmt::Write as _;
use std::io::Write;

use anyhow::{bail, Result};

/// Built-in subcommands to skip in output.
const SKIP_SUBCOMMANDS: &[&str] = &["help"];

// ── Index mode (default) ─────────────────────────────────────────────

/// Generate the agent guide and command index.
pub(crate) fn generate_index(cmd: &clap::Command, writer: &mut impl Write) -> Result<()> {
    let mut buf = String::with_capacity(4 * 1024);
    let name = cmd.get_name();

    writeln!(
        buf,
        "# {name} — Fleet GitOps YAML linter and language server"
    )?;
    writeln!(buf)?;
    writeln!(buf, "## Agent guide")?;
    writeln!(buf)?;
    writeln!(
        buf,
        "{name} is a CLI tool for linting, validating, and migrating Fleet GitOps YAML configurations."
    )?;
    writeln!(buf)?;
    writeln!(buf, "**Discovery workflow:**")?;
    writeln!(
        buf,
        "1. Read the command index below to find relevant commands"
    )?;
    writeln!(
        buf,
        "2. Run `{name} help-ai --command <name>` for full flags and usage of a specific command"
    )?;
    writeln!(
        buf,
        "3. Run `{name} help-ai --sop <tool>` for step-by-step workflows (lint, migrate, lsp, paths, history, …)"
    )?;
    writeln!(
        buf,
        "4. Run `{name} help-ai --full` for the complete reference (large output)"
    )?;
    writeln!(buf)?;
    writeln!(buf, "**JSON schema (for structured parsing):**")?;
    writeln!(buf, "- `{name} help-json` — full CLI schema as JSON")?;
    writeln!(
        buf,
        "- `{name} help-json <name>` — scoped subtree, globals stripped"
    )?;
    writeln!(buf)?;
    writeln!(buf, "**Common patterns:**")?;
    writeln!(
        buf,
        "- `--format json` on check and list-rules for structured output"
    )?;
    writeln!(
        buf,
        "- `--fix` auto-applies safe fixes, `--unsafe-fixes` for risky ones"
    )?;
    writeln!(
        buf,
        "- `{name} migrate` outputs JSON report — does NOT apply changes"
    )?;
    writeln!(
        buf,
        "- `.fleetlint.toml` configures rules, thresholds, and Fleet connection"
    )?;
    writeln!(buf)?;
    writeln!(
        buf,
        "**When to use which SOP (match user intent to the right SOP):**"
    )?;
    writeln!(
        buf,
        "- lint, validate, check, fix YAML files or a profile; offline dry-run of a merge → `--sop lint`"
    )?;
    writeln!(
        buf,
        "- migrate, upgrade, rename teams/queries/team_settings → `--sop migrate`"
    )?;
    writeln!(
        buf,
        "- editor setup (VS Code, Zed, Sublime) → `--sop lsp`"
    )?;
    writeln!(
        buf,
        "- install/remove a non-blocking pre-commit hook → `--sop hooks`"
    )?;
    writeln!(
        buf,
        "- author Fleet GitOps YAML correctly the first time → `--sop author`"
    )?;
    writeln!(
        buf,
        "- fix broken path: refs (moved files), or wire unwired profiles/scripts/software → `--sop paths`"
    )?;
    writeln!(
        buf,
        "- replay rules over git history, mine guardrails, gate rule quality in CI → `--sop history`"
    )?;
    writeln!(
        buf,
        "- generate a stanza from a .pkg/.mobileconfig/.sql etc, add/update software, policies, scripts → `--sop software`"
    )?;
    writeln!(
        buf,
        "- extend flint with a new Fleet YAML field (working ON this repo) → `--sop add-field`"
    )?;
    writeln!(buf)?;
    writeln!(
        buf,
        "All SOPs are written in **procedural format** — read top-to-bottom as PROCEDUREs"
    )?;
    writeln!(
        buf,
        "with numbered phases, ASSERTs, and explicit RETURN points. Run `{name} help-ai`"
    )?;
    writeln!(buf, "`--sop <tool>` to print one:")?;
    writeln!(
        buf,
        "- `--sop lint`        — lint workflow (config discovery → check → classify exit → CI)"
    )?;
    writeln!(
        buf,
        "- `--sop migrate`     — version migration (report → renames bottom-up → verify)"
    )?;
    writeln!(
        buf,
        "- `--sop lsp`         — editor setup for VS Code / Zed / Sublime"
    )?;
    writeln!(
        buf,
        "- `--sop hooks`       — `.git/hooks/pre-commit` install + uninstall + mode switching"
    )?;
    writeln!(
        buf,
        "- `--sop author`      — schema-as-contract loop for authoring Fleet YAML"
    )?;
    writeln!(
        buf,
        "- `--sop paths`       — fix broken path: refs (--fix) + report/wire unwired artifacts"
    )?;
    writeln!(
        buf,
        "- `--sop history`     — replay today's rules over past commits (--oracle, --suggest-patterns, --gate)"
    )?;
    writeln!(
        buf,
        "- `--sop software`    — generate stanzas from .pkg/app/profile/query; update, policy, scripts"
    )?;
    writeln!(
        buf,
        "- `--sop add-field`   — repo-extension procedure (schema → registry → docs → tests)"
    )?;
    writeln!(buf)?;

    // Command index
    writeln!(buf, "## Command index")?;
    writeln!(buf)?;

    for sub in cmd.get_subcommands() {
        if sub.is_hide_set() || SKIP_SUBCOMMANDS.contains(&sub.get_name()) {
            continue;
        }
        let about = sub.get_about().map(|a| a.to_string()).unwrap_or_default();
        writeln!(buf, "### {name} {}", sub.get_name())?;
        writeln!(buf, "{about}")?;

        // List args briefly
        for arg in sub.get_arguments() {
            if arg.is_hide_set() || arg.get_id() == "help" || arg.get_id() == "version" {
                continue;
            }
            let flag = if let Some(long) = arg.get_long() {
                format!("--{long}")
            } else if arg.is_positional() {
                format!("<{}>", arg.get_id())
            } else {
                continue;
            };
            let help = arg.get_help().map(|h| h.to_string()).unwrap_or_default();
            let req = if arg.is_required_set() {
                " (required)"
            } else {
                ""
            };
            writeln!(buf, "  {flag}{req} — {help}")?;
        }
        writeln!(buf)?;
    }

    writer.write_all(buf.as_bytes())?;
    Ok(())
}

// ── Command detail mode ──────────────────────────────────────────────

/// Generate full detail for a single command by dotted path.
pub(crate) fn generate_command(
    cmd: &clap::Command,
    dotted_path: &str,
    writer: &mut impl Write,
) -> Result<()> {
    let mut buf = String::with_capacity(2 * 1024);
    let parts: Vec<&str> = dotted_path.split('.').collect();

    let mut current = cmd;
    let mut path_parts = vec![cmd.get_name().to_string()];

    for part in &parts {
        current = current
            .get_subcommands()
            .find(|s| s.get_name() == *part)
            .ok_or_else(|| {
                let available: Vec<_> = current
                    .get_subcommands()
                    .filter(|s| !s.is_hide_set() && s.get_name() != "help")
                    .map(|s| s.get_name().to_string())
                    .collect();
                anyhow::anyhow!(
                    "Unknown command '{part}'. Available: {}",
                    available.join(", ")
                )
            })?;
        path_parts.push(current.get_name().to_string());
    }

    let full_path = path_parts.join(" ");
    let about = current
        .get_about()
        .map(|a| a.to_string())
        .unwrap_or_default();

    writeln!(buf, "# {full_path}")?;
    writeln!(buf)?;
    writeln!(buf, "{about}")?;
    writeln!(buf)?;

    if let Some(long_about) = current.get_long_about() {
        writeln!(buf, "{long_about}")?;
        writeln!(buf)?;
    }

    // Arguments
    let args: Vec<_> = current
        .get_arguments()
        .filter(|a| {
            !a.is_hide_set()
                && a.get_id() != "help"
                && a.get_id() != "version"
                && !a.is_global_set()
        })
        .collect();

    if !args.is_empty() {
        writeln!(buf, "## Arguments")?;
        writeln!(buf)?;
        for arg in args {
            write_arg_detail(&mut buf, arg)?;
        }
    }

    // Subcommands
    let subs: Vec<_> = current
        .get_subcommands()
        .filter(|s| !s.is_hide_set() && s.get_name() != "help")
        .collect();

    if !subs.is_empty() {
        writeln!(buf, "## Subcommands")?;
        writeln!(buf)?;
        for sub in subs {
            let sub_about = sub.get_about().map(|a| a.to_string()).unwrap_or_default();
            writeln!(buf, "- `{} {}` — {sub_about}", full_path, sub.get_name())?;
        }
        writeln!(buf)?;
    }

    writer.write_all(buf.as_bytes())?;
    Ok(())
}

fn write_arg_detail(buf: &mut String, arg: &clap::Arg) -> Result<()> {
    let name = arg.get_id().as_str();
    let help = arg.get_help().map(|h| h.to_string()).unwrap_or_default();

    if arg.is_positional() {
        write!(buf, "- `<{name}>`")?;
    } else if let Some(long) = arg.get_long() {
        write!(buf, "- `--{long}`")?;
        if let Some(short) = arg.get_short() {
            write!(buf, " / `-{short}`")?;
        }
    } else if let Some(short) = arg.get_short() {
        write!(buf, "- `-{short}`")?;
    } else {
        return Ok(());
    }

    if arg.is_required_set() {
        write!(buf, " **(required)**")?;
    }

    writeln!(buf, " — {help}")?;

    let defaults = arg.get_default_values();
    if !defaults.is_empty() {
        let vals: Vec<&str> = defaults.iter().filter_map(|v| v.to_str()).collect();
        writeln!(buf, "  Default: `{}`", vals.join(", "))?;
    }

    if arg.get_action().takes_values() {
        let possible: Vec<_> = arg
            .get_possible_values()
            .iter()
            .map(|v| v.get_name().to_string())
            .collect();
        if !possible.is_empty() {
            writeln!(buf, "  Values: {}", possible.join(", "))?;
        }
    }

    Ok(())
}

// ── Full mode ────────────────────────────────────────────────────────

/// Generate the complete CLI reference.
pub(crate) fn generate_full(cmd: &clap::Command, writer: &mut impl Write) -> Result<()> {
    let mut buf = String::with_capacity(8 * 1024);
    let name = cmd.get_name();

    writeln!(buf, "# {name} — complete CLI reference")?;
    writeln!(buf)?;

    for sub in cmd.get_subcommands() {
        if sub.is_hide_set() || SKIP_SUBCOMMANDS.contains(&sub.get_name()) {
            continue;
        }
        write_command_full(&mut buf, sub, name)?;
    }

    writer.write_all(buf.as_bytes())?;
    Ok(())
}

fn write_command_full(buf: &mut String, cmd: &clap::Command, parent: &str) -> Result<()> {
    let full_name = format!("{parent} {}", cmd.get_name());
    let about = cmd.get_about().map(|a| a.to_string()).unwrap_or_default();

    writeln!(buf, "## {full_name}")?;
    writeln!(buf, "{about}")?;
    writeln!(buf)?;

    let args: Vec<_> = cmd
        .get_arguments()
        .filter(|a| {
            !a.is_hide_set()
                && a.get_id() != "help"
                && a.get_id() != "version"
                && !a.is_global_set()
        })
        .collect();

    if !args.is_empty() {
        for arg in args {
            write_arg_detail(buf, arg)?;
        }
        writeln!(buf)?;
    }

    for sub in cmd.get_subcommands() {
        if sub.is_hide_set() || sub.get_name() == "help" {
            continue;
        }
        write_command_full(buf, sub, &full_name)?;
    }

    Ok(())
}

// ── SOP mode ─────────────────────────────────────────────────────────

/// Generate standard operating procedures for a specific tool.
///
/// SOPs are written in **procedural format** — they read as PROCEDUREs with
/// numbered phases, ASSERTs, IF/CASE branches, and explicit RETURN points.
/// The intent is that an AI agent can execute the steps directly without
/// needing to reverse-engineer the workflow from prose.
pub(crate) fn generate_sop(tool: &str, writer: &mut impl Write) -> Result<()> {
    let sop = match tool.to_lowercase().as_str() {
        "lint" | "check" => SOP_LINT,
        "migrate" | "migration" => SOP_MIGRATE,
        "lsp" | "editor" | "editors" => SOP_LSP,
        "hooks" | "hook" | "pre-commit" => SOP_HOOKS,
        "author" | "yaml" | "write" => SOP_AUTHOR,
        "add-field" | "addfield" | "extend" | "schema" => SOP_ADD_FIELD,
        "paths" | "path" | "unwired" | "wire" => SOP_PATHS,
        "software" | "pkg" | "app" | "profile" | "query" | "new" | "generate" => SOP_SOFTWARE,
        "history" | "replay" | "gate" | "oracle" => SOP_HISTORY,
        _ => bail!(
            "Unknown SOP: '{tool}'. Available: lint, migrate, lsp, hooks, author, add-field, paths, software, history"
        ),
    };
    writer.write_all(sop.as_bytes())?;
    Ok(())
}

const SOP_LINT: &str = r#"# SOP: Linting Fleet GitOps YAML

PROCEDURE lint_repo(path):
  ASSERT path is a directory or a YAML file (*.yml / *.yaml)

  # Phase 1 — Configuration discovery
  flint walks from <path> upward looking for `.fleetlint.toml`.
  IF NOT FOUND:
    RUN with defaults (no-op, all rules at default severity)
  IF you want repo-wide config:
    RUN: `flint init --no-interactive`        # auto-detects layout
    EDIT .fleetlint.toml (see Config block below)

  # Phase 2 — Run the linter
  RUN: `flint check <path>`
  CAPTURE: stdout + exit code
  IF path is a .mobileconfig or a DDM .json:
    `flint check <profile>` scans that one artifact directly (profile-well-formed,
    payload-uuid-format) — no fleet file needed. Wiring/reference rules still
    need the directory lint.
  SEVERITY: flint reports the coupling it can see, not the failure it cannot
    verify — a finding inside a fragment no fleet wires is a WARNING; the same
    finding inside a wired fragment is an ERROR.

  # Phase 3 — Classify exit
  IF exit == 0:
    RETURN: clean — proceed
  IF exit == 1:
    PARSE diagnostics from stdout
    FOR EACH diagnostic d:
      IF d.severity == "error":
        IF d.fix_safety == "Safe":     SUGGEST: re-run with `--fix`
        IF d.fix_safety == "Unsafe":   SUGGEST: `--fix --unsafe-fixes` + manual diff
        IF d.help is set:              the help text describes the fix
        IF d.suggestion is set:        the suggestion is the literal replacement text
      IF d is a known false positive:
        SUPPRESS_OPTIONS:
          (a) inline:    append `# flint: ignore [<rule_code>]` on the offending line
          (b) repo-wide: add to .fleetlint.toml [rules] disabled = ["<rule_code>"]
    HALT until errors == 0
  IF exit == 2:
    flint crashed — file an issue with stderr

  # Phase 4 — Machine-readable mode (CI / agents)
  RUN: `flint check <path> --format json`
  PARSE: { version, files: [{ counts, diagnostics: [{...}] }], summary }
  EXIT_CODE_CONTRACT: 0 = clean, 1 = errors, 2 = flint crash

  # Phase 5 — Whole-repo gate before push / merge (offline, no server)
  RUN: `flint dry-run <repo>`                   # verdict: would `fleetctl gitops` pass?
  EXIT_CODE_CONTRACT (dry-run): 0 = pass, 2 = blocking errors; `--strict` blocks on warnings too
  RUN: `flint dry-run <repo> --against <REF>`   # lint the MERGE of HEAD and REF, without producing it —
                                                # catches a defect that exists only in the combination
                                                # (one branch deleted files another began referencing).
                                                # A textual conflict is reported and blocks (exit 2).
  RUN: `flint dry-run <repo> --oracle <PATH>`   # dev/CI only (`gitops-oracle`, not shipped): audits the
                                                # blocking findings against Fleet's own parser —
                                                # "expected" = Fleet enforces it server-side,
                                                # "REVIEW" = possible false positive. Advisory: never
                                                # changes the verdict or exit code.
  A clean dry-run means flint found nothing — weaker than fleetctl accepting the apply.

  # Phase 6 — Inspect what flint enforces
  RUN: `flint list-rules`              # table view
  RUN: `flint list-rules --format json` # for programmatic use

# Configuration (.fleetlint.toml)
```toml
[rules]
disabled = ["secret-hygiene"]        # silence specific rules entirely
warn = ["interval-validation"]       # downgrade errors to warnings

[thresholds]
min_interval = 60                    # query interval bounds (seconds)
max_interval = 86400

[files]
# include unset = everything not excluded. If you narrow it, use DIRECTORY
# globs ("platforms/**"), never extension globs — a non-empty include is
# authoritative and also scopes the cross-file rules, which report on
# scripts/profiles, so "**/*.yml" would silently disable them.
exclude = ["node_modules", "target"]

[deprecations]
fleet_version = "4.85.0"             # target version for deprecation checks
future_names = true                  # opt in to new naming (reports, settings, fleets)

[orphans]
# Artifacts kept on purpose although no fleet references them. Silences
# `orphaned-file` for the match and nothing else — unlike [files] exclude,
# which would take the file out of scope for every rule.
allow = ["profiles/archive/**"]

[fleet.env]
# Names this repo supplies to fleetctl. `env-var-resolvable` warns on any
# `${VAR}` that is neither set here, in the environment, nor server-resolved
# (`$FLEET_SECRET_*`, `$FLEET_VAR_*`). fleetctl expands the RAW file text,
# comments included, so one unset name fails the whole file.
FLEET_BOOTSTRAP_TOKEN = "op://Vault/Item/field"
```

# Key flags
- `--format json`        — structured output (CI, scripts, agents)
- `--fix`                — auto-apply Safe fixes (renames, typo corrections)
- `--unsafe-fixes`       — also apply Unsafe fixes (requires `--fix`)
- `--hook-mode`          — always exit 0 (used by `flint hooks install`)
"#;

const SOP_MIGRATE: &str = r#"# SOP: Fleet GitOps Migration

PROCEDURE migrate_repo(path, target_version):
  ASSERT path is a Fleet GitOps repo root
  ASSERT target_version is a Fleet release (e.g. "4.85.0" or "latest")

  # Phase 1 — Generate the migration report (read-only)
  RUN: `flint migrate <path> --target-version <version>` -> JSON
  PARSE: {
    summary: { files_scanned, directory_renames, file_renames, key_renames, safe_fixes },
    directory_renames: [{ old, new, files_affected }],
    file_renames:      [{ old, new }],
    file_changes:      [{ path, move_to?, key_renames: [{ line, old_key, new_key, safety }] }],
  }
  ASSERT summary.files_scanned > 0

  # Phase 2 — Directory renames FIRST (other phases reference the new paths)
  FOR EACH dr IN directory_renames:
    RUN: `mv <path>/<dr.old> <path>/<dr.new>`
    ASSERT <path>/<dr.old> no longer exists

  # Phase 3 — File renames
  FOR EACH fr IN file_renames:
    RUN: `mv <path>/<fr.old> <path>/<fr.new>`
    NOTE: a file inside a renamed directory uses the new directory name

  # Phase 4 — In-file key renames (apply bottom-up to preserve line offsets)
  FOR EACH fc IN file_changes:
    target := fc.move_to OR fc.path
    SORT fc.key_renames BY line DESC
    FOR EACH kr IN fc.key_renames:
      REPLACE first occurrence of "<kr.old_key>:" at line <kr.line> in <target>
              with "<kr.new_key>:"
      IF kr.safety == "warning":
        REVIEW: rename has semantic implications — human eyeball recommended

  # Phase 5 — Cross-file path references
  GREP for old names that may appear inside YAML `path:` / `paths:` values:
    `grep -rn "<old_dir>/" <path>/**/*.yml`
    `grep -rn "<old_file>" <path>/**/*.yml`
  REWRITE matches manually, or via sed for the obvious cases

  # Phase 6 — Verify
  RUN: `flint check <path>`
  ASSERT no deprecation warnings remain
  RETURN: clean diff ready for PR

# Current renames (warnings since v4.80.1)
- Directory: `teams/`          -> `fleets/`
- File:      `no-team.yml`     -> `unassigned.yml`
- Key:       `team_settings:`  -> `settings:`
- Key:       `queries:`        -> `reports:`  (top-level only)
"#;

const SOP_LSP: &str = r#"# SOP: Editor Setup (LSP)

PROCEDURE setup_editor(editor):
  ASSERT editor IN {vscode, zed, sublime}
  ASSERT `flint --version` succeeds (binary on PATH)

  CASE editor:
    vscode:
      INSTALL: VSIX from the GitHub release (`flint-<v>.vsix`) or the
               marketplace listing (when published).
      LOAD a Fleet GitOps YAML and verify diagnostics + hover appear.

    zed:
      EDIT  ~/.config/zed/settings.json:
        ADD `"language_servers": ["flint-lsp"]` for the YAML language entry
        DISABLE the default yaml-language-server for Fleet YAML files
      INSTALL the extension via the Zed extension registry, OR sideload
              `flint-zed-<v>.zip` from the GitHub release.

    sublime:
      INSTALL Package Control
      INSTALL the `LSP` package
      INSTALL the `LSP-flint` package — its plugin auto-pulls
              `flint-<v>-darwin-arm64.tar.gz` from the latest GitHub release
              on first activation.

  # Verification (same for every editor)
  OPEN any Fleet GitOps YAML
  ASSERT hover on a documented field shows description + valid values
  ASSERT a typo in a known key surfaces a diagnostic in real time
  ASSERT `path:` value completion offers files in the workspace

# Capabilities (what the LSP provides)
- diagnostics       — real-time, every keystroke
- hover             — field/table docs, valid values, examples
- completion        — keys, platform values, osquery tables, common labels, paths/globs
- code actions      — quick fixes for deprecated keys + typos
- go-to-definition  — `path:` references, label name references
- document symbols  — outline of the YAML
- semantic tokens   — SQL syntax inside `query:` values

# Configuration
The LSP reads `.fleetlint.toml` (same format as `flint check`):
- [rules]        — disabled / warn lists
- [thresholds]   — query interval bounds
- [deprecations] — target Fleet version, future_names opt-in
"#;

const SOP_HOOKS: &str = r#"# SOP: Pre-commit git hooks

PROCEDURE install_hook(repo_root, mode):
  ASSERT repo_root contains `.git/`
  ASSERT mode IN {non-blocking, strict, json, strict+json}

  CD repo_root

  CASE mode:
    non-blocking:   `flint hooks install`                  # default
    strict:         `flint hooks install --strict`         # errors block commits
    json:           `flint hooks install --json`           # structured output
    strict+json:    `flint hooks install --strict --json`  # block + JSON

  IF an existing pre-commit hook is present and not authored by flint:
    flint refuses to overwrite — re-run with `--force` to overwrite, or
    move the existing hook aside first.

  ASSERT `.git/hooks/pre-commit` is now executable (chmod 755).

PROCEDURE remove_hook(repo_root):
  CD repo_root
  RUN: `flint hooks uninstall`
  ASSERT it refuses if the existing hook was not authored by flint.

# Behavior contract (what the generated hook does)
- Runs `flint check` only on staged YAML files (or the whole repo if none staged).
- Non-blocking (default): always exits 0 — diagnostics print but commits proceed.
- Strict: flint's native exit code propagates. Bypass with `git commit --no-verify`.
- JSON: emits structured diagnostics suitable for CI piping or `jq` parsing.
- Auto-skips if `flint` is not on PATH (logs a single warning to stderr).

# How to switch modes later
  RUN: `flint hooks install --force <new flags>`     # overwrite in place
  e.g. switch from non-blocking to strict:
       `flint hooks install --force --strict`

# How to debug
  cat .git/hooks/pre-commit | head -10
  → the comment header records `Strict mode: <bool>. JSON output: <bool>.`
"#;

const SOP_AUTHOR: &str = r#"# SOP: Authoring Fleet GitOps YAML against flint's schema

PROCEDURE author_fleet_yaml(target_file, intent):
  # Audience: any agent (or human) writing Fleet GitOps YAML in any repo.

  # Phase 1 — Identify the file kind from its path (flint auto-detects)
  CASE target_file:
    *default.yml              -> top-level FleetConfig (org_settings, controls, …)
    fleets/**/*.yml           -> per-fleet config (settings.webhook_settings allowed)
    fleets/unassigned.yml     -> "Unassigned" fleet (alias of a per-fleet file)
    labels/**/*.yml           -> standalone label array (top-level sequence)
    */policies/**/*.yml       -> standalone policy array (top-level sequence)
    */queries/**/*.yml        -> standalone query/report array
    */software/**/*.yml       -> standalone software-package config
    agent-options*.yml        -> standalone agent_options config

  # Phase 2 — Consult the schema BEFORE writing
  REFERENCE: editors/vscode/schemas/<kind>.schema.json
             (mirrors the rules `flint check` will apply)
  REFERENCE for canonical Fleet keys:
    /Users/henry/Code/GitHub/fleet/docs/Configuration/yaml-files.md
    /Users/henry/Code/GitHub/fleet/docs/REST API/rest-api.md
    /Users/henry/Code/GitHub/fleet/cmd/fleetctl/fleetctl/testdata/generateGitops/

  # Phase 3 — Draft
  WRITE the YAML following the schema for <kind>
  AVOID keys not in the schema (`flint check` will flag them)
  PREFER canonical names over the deprecated forms:
    `fleets/`          NOT `teams/`
    `settings:`        NOT `team_settings:`
    `reports:`         NOT `queries:`           (top-level only)
    `unassigned.yml`   NOT `no-team.yml`

  # Phase 4 — Validate
  RUN: `flint check <target_file>`
  IF errors > 0:
    READ each diagnostic — `help` describes the issue, `suggestion` gives the fix
    LOOP back to Phase 3
  IF warnings > 0:
    EVALUATE each — they are typically not blocking but worth fixing

  # Phase 5 — Cross-file gate
  IF target_file references other files via `path:` or `paths:`:
    ASSERT each referenced file exists relative to the current file
    RUN: `flint check <repo_root>` to validate the whole graph

  # Phase 6 — Commit gate
  IF the repo has a flint pre-commit hook installed:
    `git commit` will run `flint check --hook-mode` automatically.
  ELSE:
    OPTIONAL: install one — see `flint help-agents --sop hooks`.

# What flint catches that a plain YAML linter does not
- misplaced keys (e.g. `webhooks_and_tickets_enabled` under `integrations:`)
- multi-platform false positives (`platform: darwin,linux` is split correctly)
- patch-policy coupling (`type: patch` requires `fleet_maintained_app_slug`)
- per-fleet vs org-only constraints (`vulnerabilities_webhook` is org-only)
- deprecated keys with a fix suggestion (`teams:` → `fleets:`)
- osquery table availability per platform (e.g. `usb_devices` is darwin+linux only)
"#;

const SOP_ADD_FIELD: &str = r#"# SOP: Add a new Fleet YAML field to flint
# Audience: agents working ON this repo (extending the schema/linter).

PROCEDURE add_fleet_field(field_name, parent_path, field_type, valid_values?):
  # parent_path uses dot notation:
  #   "controls"             "policies[]"           "labels[].criteria"
  #   "agent_options"        "software.packages[]"  "team_settings.webhook_settings"

  # Phase 1 — Source of truth
  REFERENCE one or more of:
    /Users/henry/Code/GitHub/fleet/docs/Configuration/yaml-files.md
    /Users/henry/Code/GitHub/fleet/docs/REST API/rest-api.md
    /Users/henry/Code/GitHub/fleet/pkg/spec/gitops.go
    /Users/henry/Code/GitHub/fleet/server/fleet/*.go
    /Users/henry/Code/GitHub/fleet/cmd/fleetctl/fleetctl/testdata/generateGitops/
  ASSERT field_name + parent_path are confirmed by at least one source.

  # Phase 2 — Structural schema
  EDIT crates/flint-lint/src/structure.rs
  LOCATE the helper that builds <parent_path>'s children
         (e.g. policy_inline_strict, controls_schema, agent_options_inline)
  ADD: ("<field_name>", <node>)
       where <node> is one of:
         leaf()                       — opaque scalar
         boolean_leaf()               — strict bool
         array(item_node)             — sequence
         mapping(vec![...])           — fixed-key object
         open_mapping()               — additionalProperties: true

  # Phase 3 — Key registry (typo + misplaced-key detection)
  EDIT same file (KEY_REGISTRY block)
  ADD: reg.register("<field_name>", "<parent_path>");
  IF the key is also valid under additional parents:
    REGISTER under each — KeyRegistry stores a Vec<&str> per key.

  # Phase 4 — Hover docs (LSP)
  EDIT crates/flint-lsp/src/schema.rs (FIELD_DOCS HashMap)
  INSERT m.insert(
    "<parent_path>.<field_name>",
    FieldDoc {
      name: "<field_name>",
      description: "...",
      valid_values: <Option<&[&str]>>,
      example: Some("<field_name>: <value>"),
      required: <bool>,
      field_type: "<rendered type>",
      cli_hint: None,
    },
  );

  # Phase 5 — Completion (LSP)
  EDIT crates/flint-lsp/src/completion.rs
  LOCATE the matching `complete_*_fields` function
  ADD a tuple: ("<field_name>", "<short description>", <required: bool>)
  IF field has an enum:
    ADD a value-position branch in the same function that returns the enum.

  # Phase 6 — JSON Schemas (used by VS Code's yaml-language-server)
  IF a per-kind schema exists for this parent:
    EDIT editors/vscode/schemas/<kind>.schema.json
    EDIT .vscode/fleet-gitops-schema/<kind>.schema.json   (mirror)

  # Phase 7 — Tests (regression guard)
  EDIT one of:
    crates/flint-lint/src/structural.rs   (structural acceptance/rejection)
    crates/flint-lint/src/semantic.rs     (semantic coupling rules)
    crates/flint-lint/src/engine.rs       (end-to-end via Linter)
  ADD a test using the new field in a realistic snippet.
  ASSERT no errors (positive case) AND a clear error for misuse (negative case).

  # Phase 8 — Validate
  RUN: `cargo test --workspace`
  RUN: `cargo clippy -p flint-lint --lib --no-deps -- -D warnings`
  RUN: `cargo clippy -p flint-lsp --lib --no-deps -- -D warnings`
  RUN: `cargo build --release -p flint`
  RUN: `target/release/flint check <fixture-with-the-new-field>` -> expect clean

  # Phase 9 — End-to-end on a real repo
  RUN: `target/release/flint check <your-gitops-repo>`
  ASSERT no NEW errors vs the pre-change baseline.

  # Phase 10 — Bump (only if shipping)
  See `flint help-agents --sop release` (separate procedure):
    bump version in 6 spots, regenerate Cargo.lock, run release script.

# Anti-patterns
- Adding a field only to FIELD_DOCS or only to structure.rs → users get
  inconsistent UX (hover but no completion, or completion but no validation).
- Forgetting KEY_REGISTRY → typo suggestions become useless for the new key.
- Skipping a regression test → the next refactor will silently break it.
"#;

const SOP_PATHS: &str = r#"# SOP: Path references — fix broken refs & wire unwired artifacts

Two inverse problems over the repo's `path:`/`paths:` dependency graph:
  A. broken references — a `path:` (or a policy's
     `install_software.package_path:`) whose target file is missing (often a
     moved file after a folder reorg).
  B. unwired artifacts — a file that exists but NO fleet/team config references.

PROCEDURE fix_broken_paths(dir):
  # Phase 1 — Report
  RUN: `flint paths <dir>`
  READS as before→after blocks grouped by file:
    L<line>  - <old path>   (not found)
             + <new path>   (moved here)   # unique basename match → auto-fixable
             ↳ renamed in <sha> "<subject>"           # git confirms the move
    L<line>  ? <old path>   (reason)        # ambiguous or genuinely missing, then what git knows:
             renamed to <new> in <sha> "<subject>"    # recorded rename → --fix rewrites it
             deleted in <sha> "<subject>"             # gone: drop the reference or restore the file
  ALSO surfaced as the `path-exists` lint rule in `flint check` and the editor.
  SEVERITY: inside a fragment no fleet wires, path-exists / path-is-file are
    WARNINGS (flint reports the coupling it can see); wired → ERROR.

  # Phase 2 — Apply
  RUN: `flint paths <dir> --fix`
  EFFECT: rewrites every UNAMBIGUOUS reference in place — a single basename
  match on disk, OR a rename git recorded (followed up to 5 hops: old → mid → new).
  Ambiguous (multiple candidates) / deleted / missing are left for manual fix.
  In the editor: a quick-fix lightbulb offers each candidate; a "Fix all N
  references to <file>" action repairs every referrer across the workspace.

PROCEDURE wire_unwired(dir):
  # Phase 1 — Report orphans
  RUN: `flint paths <dir> --unwired`
  Groups artifacts no config references, by directory, each routed to the
  GitOps section that would wire it (current Fleet keys):
    .mobileconfig / DDM .json → controls.apple_settings.configuration_profiles
    .xml (Windows CSP)        → controls.windows_settings.configuration_profiles
    .sh / .ps1                → controls.scripts
    enrollment .dep.json      → controls.setup_experience.apple_setup_assistant
    software/*.yml            → software.packages
    policies|queries|reports  → policies / queries
  Prints copy-paste `paths:` glob + individual `path:` lines, relative to where
  THIS repo's fleet files live (not a hardcoded ../).

  # Phase 2 — Interactive wiring (modifies files only on explicit yes)
  RUN: `flint paths <dir> --unwired --interactive`
  PER GROUP it lists each artifact (name/scope/payload-type), then PER FLEET asks
  [y]es / [a]ll-remaining / [n]o / [s]kip group / [q]uit.
    [y] → choose [g]lob (one rule, no labels) or [p]er-file (per-path labels)
    [a] → wire the glob into this and every remaining fleet at once
  PER-FILE prompts labels_include_any AND labels_exclude_any (independent,
  combinable: broad include + targeted exclude).
  FLAGS:
    --label-stubs[=blank|comment]  blank labels → emit the empty key (=blank,
                                   default) or a commented stub (=comment)
    --only <glob>                  limit target fleets (e.g. "fleets/team-*.yml")

# Key facts
- path: is a single file; paths: is a glob (`*`, `?`, `**`). A glob cannot be
  label-scoped — per-path labels require the per-file style.
- An artifact kept on purpose (archive, template) is declared, not excluded:
  `.fleetlint.toml` → `[orphans] allow = ["profiles/archive/**"]` silences
  `orphaned-file` for it and nothing else.
- Inserts land after the last REAL entry of the section (never below trailing
  commented-out blocks), and convert an inline `key: []` to a block first.
"#;

const SOP_SOFTWARE: &str = r#"# SOP: Generate Fleet GitOps stanzas from real artifacts

Read a real file and emit the matching GitOps YAML (no hand-transcription).
All generators support `-o <file>` to append; profiles/software support `--wire`
to insert interactively into a chosen fleet (run from INSIDE your GitOps repo —
software is url-based so the installer may live anywhere, e.g. ~/Downloads).

PROCEDURE add_software_package(installer):
  # macOS .pkg
  RUN: `flint gen software --from <file.pkg>`            # minimal: comment header + hash_sha256
  RUN: `flint gen software --from <file.pkg> --full`     # full stanza: url placeholder, self_service,
                                                         # commented display_name/categories/labels/scripts
  ADD `setup_experience: true` (install during Setup Assistant / OOBE):
    `flint gen software --from <file.pkg> --full --setup-experience`
  WIRE into a fleet (prompts for setup_experience unless --setup-experience given):
    `flint gen software --from <file.pkg> --wire`
  WRITE a standalone software file (the `path:`-referenced mapping form):
    `flint gen software --from <file.pkg> --standalone`            # → <name>.package.yml next to the .pkg
    `flint gen software --from <file.pkg> --standalone --full -o <dir>`   # full scaffolding, random name in <dir>
    `flint gen software --from <file.pkg> --standalone -o <name>.yml`     # exact filename (refuses overwrite)
  EFFECT: name derived from the installer (`<name>.package.yml`), -2/-3 on collision; honors --full and
  --setup-experience. hash_sha256 leads the file; url is a fill-in placeholder.
  OTHER installer formats (same command, dispatch by extension):
    `flint gen software --from <file>`   # .deb (ar+tar), .ipa (unzip+plutil), .msi (msiinfo),
                                         # .rpm (rpm), .exe/.tar.gz (hash + placeholders)
                                         # .dmg is NOT a Fleet format — repackage to .pkg.
  BATCH a directory: `flint gen software --from <dir>` / `flint gen profile --from <dir>`.

PROCEDURE keep_a_package_current(installer, software_file):
  # New release of an already-referenced package:
  RUN: `flint gen software --from <new.pkg> --update <software_file>`
  EFFECT: matches the stanza by package identifier and refreshes hash_sha256,
  version (header), and the url filename — leaving comments/labels/self_service intact.

PROCEDURE generate_install_check_policy(pkg):
  RUN: `flint gen policy --from <file.pkg>`           # via package_receipts (package_id+version)
  RUN: `flint gen policy --from <file.pkg> --apps`    # via apps table (bundle_identifier)
  RUN: `flint gen policy --from <file.pkg> --enforce` # + install_software (auto-install on fail)
  Uses osquery's semantic version_compare (>= target). No version → existence check.
  The plain form is an AUDIT policy (reports drift). --enforce appends an
  install_software.hash_sha256 automation so Fleet auto-installs the package on
  failing hosts; the package must also be listed under software (same hash).

PROCEDURE default_install_uninstall_scripts(pkg):
  RUN: `flint gen scripts --from <file.pkg> -o <dir>`
  Writes Fleet's VERBATIM default install.sh + uninstall.sh ($PACKAGE_ID is
  substituted by the Fleet server at upload; do not hardcode it).

PROCEDURE add_configuration_profile(file):
  RUN: `flint gen profile --from <file.mobileconfig>`  # → configuration_profiles entry
       # header: display name — identifier [scope] · nested PayloadType
  RUN: `flint gen profile --from <file.json>`          # DDM declaration (Type + Identifier)
  RUN: `flint gen profile --from <file.xml>`           # Windows CSP (LocURI context)
  RUN: `flint gen profile --from <enroll.dep.json>`    # → setup_experience.apple_setup_assistant
  FLAGS: --full (label stubs), --wire (insert into a fleet, path per-fleet-relative)
  MITIGATE a duplicate-payload-uuid finding:
    `flint gen profile --from <file.mobileconfig> --regen-uuid`   # mints a fresh PayloadUUID

PROCEDURE add_query_or_policy(sql):
  RUN: `flint gen query --from <file.sql>`    # query stanza; platform inferred from the
                                              # osquery tables referenced (intersection)
  RUN: `flint gen policy --from <file.sql>`   # policy stanza around that query
  Unknown tables → platform left as a commented placeholder to fill in.

PROCEDURE scaffold_new(kind):
  RUN: `flint gen <profile|fleet|policy|query|label>`  # blank template; -o to write
  `gen profile` (no --from) mints a fresh PayloadUUID (uuidgen).

# Fleet Maintained Apps (FMA) — referenced by slug, no file to scan
- software.fleet_maintained_apps entries use `slug: <app>/<platform>` (e.g.
  `slug: santa/darwin`), optionally `setup_experience: true`, labels, version.
- An inline comment after the slug is fine: `slug: santa/darwin # Santa`.
"#;

const SOP_HISTORY: &str = r#"# SOP: Rule history — replay, mine, diff, gate

Four questions over the repo's git history, all read-only (each commit is
reconstructed into a scratch directory; the working copy is never touched):
  A. replay   — which rules WOULD have fired on past commits, and how often?
  B. mine     — which conventions were repaired by hand, repeatedly, with no rule?
  C. diff     — where does flint's blocking verdict disagree with Fleet's parser?
  D. gate     — has rule quality regressed since the stored baseline? (CI)

PROCEDURE replay(repo):
  RUN: `flint history <repo> --max 400`        # last 400 first-parent commits (default 200)
  OR:  `flint history <repo> --since <REF>`    # REF (exclusive) .. HEAD
  READS per rule code, ranked:
    <code>  ×N closed windows, <M> commit(s)
        <sha> <subject>  →  fixed in <sha> <subject>
        <sha> <subject>  →  still firing at HEAD
  A "red window" is a run of commits in which the code fired; it CLOSES at the
  commit that made it stop. Only CLOSED windows score — an open one describes
  current state, not a repeated mistake. ×2 or more closed = repeat failure,
  which is the ranking that matters.
  "Not replayed" lists codes that cannot be judged from history (software-source
  depends on server state at that commit, which is gone).
  FLAGS:
    --scope-as-committed   use each tree's OWN .fleetlint.toml instead of today's
                           ("what would flint have said at the time")
    --no-reuse             recompute every commit (reuse of unchanged trees is
                           output-identical; this proves it or bisects a difference)
    --json                 the form an agent consumes

PROCEDURE suggest_patterns(repo):
  RUN: `flint history <repo> --suggest-patterns [--min-occurrences 2]`
  EFFECT: mines commits that look like remediation and proposes `[[patterns]]`
  guardrails for conventions repaired at least N times (one repair is an anecdote).
  OUTPUT is heuristic, emitted COMMENTED OUT, and never written to any config.
  FOR EACH suggestion: REVIEW it, THEN paste into `.fleetlint.toml` `[[patterns]]`,
  THEN `flint check <repo>` to confirm it fires where intended.

PROCEDURE oracle_diff(repo, oracle):
  ASSERT oracle is a built `gitops-oracle` binary (dev/CI only — not shipped)
  RUN: `flint history <repo> --oracle <oracle>`
  EFFECT: each replayed tree also goes through Fleet's own parser
  (`spec.GitOpsFromFile`, what `fleetctl gitops` calls) and blocking claims are
  diffed BOTH ways:
    flint blocked, Fleet accepted — per rule code, each marked
      "expected — <why>"   Fleet enforces it server-side, outside the parser:
                           profile-well-formed, duplicate-fleet-name, software-source
      "REVIEW"             possible false positive — a false claim sends someone
                           to edit working config; fix the rule or narrow it
    Fleet blocked, flint silent — the gap list, measured rather than argued
  For a single tree (a PR, not history) use `flint dry-run --oracle <oracle>`.

PROCEDURE gate(repo, baseline):
  RUN: `flint history <repo> --gate <baseline.json> [--oracle <oracle>]`
  IF baseline does not exist: it is written and the run PASSES
  ELSE: exit 2 IF a rule NEWLY blocks where Fleet accepts, or Fleet complains
        about something flint has NEWLY gone silent on, or a rule no longer
        catches what it used to.
  Only new KEYS gate. Counts move with the range replayed, so they are reported
  and never failed on.
  IF a change is understood and intended:
    RUN: `flint history <repo> --gate <baseline.json> --update-baseline`
    COMMIT the baseline with the rule change that explains it.
  NEVER update the baseline to silence a gate you have not explained.

# Key facts
- Default replay applies TODAY'S config to every tree, so scope is held fixed;
  a directory the repo has since declared out of scope cannot pollute history.
- Reconstruction uses `git archive` per commit — read-only, safe on a dirty tree.
- `--json` on every mode; `--gate` exits 2 on regression, 0 otherwise.
"#;

// ── JSON mode ────────────────────────────────────────────────────────

/// Generate JSON schema of the CLI.
/// If `path` is provided, scopes to that subtree with global flags stripped.
pub(crate) fn generate_json(
    cmd: &clap::Command,
    path: Option<&str>,
    writer: &mut impl Write,
) -> Result<()> {
    let json = if let Some(path) = path {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = cmd;
        for part in &parts {
            current = current
                .get_subcommands()
                .find(|s| s.get_name() == *part)
                .ok_or_else(|| {
                    let available: Vec<_> = current
                        .get_subcommands()
                        .filter(|s| !s.is_hide_set() && s.get_name() != "help")
                        .map(|s| s.get_name().to_string())
                        .collect();
                    anyhow::anyhow!(
                        "Unknown command '{part}'. Available: {}",
                        available.join(", ")
                    )
                })?;
        }
        command_to_json_no_globals(current)
    } else {
        command_to_json(cmd)
    };
    let output = serde_json::to_string_pretty(&json)?;
    writer.write_all(output.as_bytes())?;
    writeln!(writer)?;
    Ok(())
}

fn command_to_json(cmd: &clap::Command) -> serde_json::Value {
    let args: Vec<serde_json::Value> = cmd
        .get_arguments()
        .filter(|a| !a.is_hide_set() && a.get_id() != "help" && a.get_id() != "version")
        .map(arg_to_json)
        .collect();

    let subcommands: Vec<serde_json::Value> = cmd
        .get_subcommands()
        .filter(|s| !s.is_hide_set() && s.get_name() != "help")
        .map(command_to_json)
        .collect();

    let mut obj = serde_json::json!({
        "name": cmd.get_name(),
        "about": cmd.get_about().map(|a| a.to_string()),
    });

    if let Some(version) = cmd.get_version() {
        obj["version"] = serde_json::json!(version);
    }

    if !args.is_empty() {
        obj["args"] = serde_json::json!(args);
    }

    if !subcommands.is_empty() {
        obj["subcommands"] = serde_json::json!(subcommands);
    }

    obj
}

fn command_to_json_no_globals(cmd: &clap::Command) -> serde_json::Value {
    let args: Vec<serde_json::Value> = cmd
        .get_arguments()
        .filter(|a| {
            !a.is_hide_set()
                && a.get_id() != "help"
                && a.get_id() != "version"
                && !a.is_global_set()
        })
        .map(arg_to_json)
        .collect();

    let subcommands: Vec<serde_json::Value> = cmd
        .get_subcommands()
        .filter(|s| !s.is_hide_set() && s.get_name() != "help")
        .map(command_to_json_no_globals)
        .collect();

    let mut obj = serde_json::json!({
        "name": cmd.get_name(),
        "about": cmd.get_about().map(|a| a.to_string()),
    });

    if !args.is_empty() {
        obj["args"] = serde_json::json!(args);
    }

    if !subcommands.is_empty() {
        obj["subcommands"] = serde_json::json!(subcommands);
    }

    obj
}

fn arg_to_json(arg: &clap::Arg) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "name": arg.get_id().as_str(),
        "required": arg.is_required_set(),
        "positional": arg.is_positional(),
    });

    if let Some(long) = arg.get_long() {
        obj["long"] = serde_json::json!(format!("--{long}"));
    }

    if let Some(short) = arg.get_short() {
        obj["short"] = serde_json::json!(format!("-{short}"));
    }

    if let Some(help) = arg.get_help() {
        obj["help"] = serde_json::json!(help.to_string());
    }

    let defaults = arg.get_default_values();
    if !defaults.is_empty() {
        let vals: Vec<&str> = defaults.iter().filter_map(|v| v.to_str()).collect();
        obj["default"] = serde_json::json!(vals.join(", "));
    }

    if arg.get_action().takes_values() {
        let possible: Vec<_> = arg
            .get_possible_values()
            .iter()
            .map(|v| v.get_name().to_string())
            .collect();
        if !possible.is_empty() {
            obj["possible_values"] = serde_json::json!(possible);
        }
    }

    if arg.is_global_set() {
        obj["global"] = serde_json::json!(true);
    }

    obj
}

// ── Skill file installation ──────────────────────────────────────────

const SKILL_FLINT: &str = include_str!("../skills/flint.md");
const SKILL_FLEET_MIGRATE: &str = include_str!("../skills/fleet-migrate.md");

/// Install Claude Code skill files for flint.
///
/// Creates `.claude/skills/flint.md` and `.claude/skills/fleet-migrate.md`,
/// then ensures `CLAUDE.md` has a bootstrap hint.
pub(crate) fn install_skill(version: &str) -> Result<()> {
    use std::fs;
    use std::path::Path;

    // 1. Install skill files
    let skills_dir = Path::new(".claude/skills");
    fs::create_dir_all(skills_dir)?;

    for (filename, template) in &[
        ("flint.md", SKILL_FLINT),
        ("fleet-migrate.md", SKILL_FLEET_MIGRATE),
    ] {
        let skill_path = skills_dir.join(filename);
        let content = template.replace("{{VERSION}}", version);
        fs::write(&skill_path, &content)?;
        eprintln!("\u{2713} Installed Claude Code skill: .claude/skills/{filename}");
    }

    // 2. Ensure CLAUDE.md has a flint bootstrap hint
    let bootstrap_line = "## flint\n\n\
        `flint` (Fleet GitOps linter) is available. \
        Run `flint setup-agent` to install the AI agent skill, \
        or `flint help-ai` for the command reference.\n";

    let claude_md = Path::new("CLAUDE.md");
    if claude_md.exists() {
        let existing = fs::read_to_string(claude_md)?;
        if !existing.contains("flint setup-agent") && !existing.contains("flint help-ai") {
            let mut updated = existing;
            if !updated.ends_with('\n') {
                updated.push('\n');
            }
            updated.push('\n');
            updated.push_str(bootstrap_line);
            fs::write(claude_md, updated)?;
            eprintln!("\u{2713} Added flint bootstrap hint to CLAUDE.md");
        } else {
            eprintln!("  CLAUDE.md already references flint");
        }
    } else {
        let content = format!("# Project Instructions\n\n{bootstrap_line}");
        fs::write(claude_md, content)?;
        eprintln!("\u{2713} Created CLAUDE.md with flint bootstrap hint");
    }

    eprintln!("  Agents will now discover flint automatically.");
    eprintln!("  Regenerate anytime with: flint setup-agent");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Arg, Command};

    fn sample_cmd() -> Command {
        Command::new("flint")
            .version("0.1.0")
            .about("Test tool")
            .subcommand(
                Command::new("check")
                    .about("Lint YAML files")
                    .arg(Arg::new("path").required(true))
                    .arg(
                        Arg::new("fix")
                            .long("fix")
                            .action(clap::ArgAction::SetTrue)
                            .help("Auto-fix"),
                    ),
            )
            .subcommand(
                Command::new("migrate")
                    .about("Generate migration report")
                    .arg(Arg::new("path").required(true)),
            )
    }

    #[test]
    fn test_generate_index() {
        let cmd = sample_cmd();
        let mut out = Vec::new();
        generate_index(&cmd, &mut out).unwrap();
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("# flint"));
        assert!(output.contains("## Command index"));
        assert!(output.contains("check"));
        assert!(output.contains("migrate"));
    }

    #[test]
    fn test_generate_command() {
        let cmd = sample_cmd();
        let mut out = Vec::new();
        generate_command(&cmd, "check", &mut out).unwrap();
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("# flint check"));
        assert!(output.contains("--fix"));
    }

    #[test]
    fn test_generate_command_not_found() {
        let cmd = sample_cmd();
        let mut out = Vec::new();
        let result = generate_command(&cmd, "nonexistent", &mut out);
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_sop_lint() {
        let mut out = Vec::new();
        generate_sop("lint", &mut out).unwrap();
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("SOP: Linting"));
        // Procedural format: every SOP must read as a PROCEDURE.
        assert!(
            output.contains("PROCEDURE "),
            "lint SOP should be in procedural format"
        );
    }

    #[test]
    fn test_all_sops_use_procedural_format() {
        for tool in [
            "lint",
            "migrate",
            "lsp",
            "hooks",
            "author",
            "add-field",
            "paths",
            "software",
            "history",
        ] {
            let mut out = Vec::new();
            generate_sop(tool, &mut out)
                .unwrap_or_else(|e| panic!("SOP '{tool}' missing: {e}"));
            let output = String::from_utf8(out).unwrap();
            assert!(
                output.contains("PROCEDURE "),
                "SOP '{tool}' should be in procedural format (must contain PROCEDURE)"
            );
        }
    }

    #[test]
    fn test_generate_sop_aliases_resolve() {
        // Each SOP should be reachable by at least one ergonomic alias.
        for alias in [
            "check",       // lint
            "migration",   // migrate
            "editor",      // lsp
            "pre-commit",  // hooks
            "yaml",        // author
            "addfield",    // add-field
            "unwired",     // paths
            "generate",    // software
            "replay",      // history
        ] {
            let mut out = Vec::new();
            generate_sop(alias, &mut out)
                .unwrap_or_else(|e| panic!("alias '{alias}' should resolve: {e}"));
            assert!(!out.is_empty(), "alias '{alias}' returned empty output");
        }
    }

    #[test]
    fn test_generate_sop_unknown() {
        let mut out = Vec::new();
        let result = generate_sop("nonexistent", &mut out);
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_json() {
        let cmd = sample_cmd();
        let mut out = Vec::new();
        generate_json(&cmd, None, &mut out).unwrap();
        let output = String::from_utf8(out).unwrap();
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(json["name"], "flint");
        assert!(json["subcommands"].is_array());
    }

    #[test]
    fn test_generate_json_scoped() {
        let cmd = sample_cmd();
        let mut out = Vec::new();
        generate_json(&cmd, Some("check"), &mut out).unwrap();
        let output = String::from_utf8(out).unwrap();
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(json["name"], "check");
    }
}
