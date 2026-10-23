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
use std::path::Path;

use anyhow::{bail, Result};

/// Built-in subcommands to skip in output.
const SKIP_SUBCOMMANDS: &[&str] = &["help"];

// ── Index mode (default) ─────────────────────────────────────────────

/// Generate the agent guide and command index.
/// The agent guide: discovery, common patterns and SOP routing, then one line
/// per command (full flags: `--command <name>`).
const AGENT_GUIDE: &str = r#"# flint — Fleet GitOps YAML linter and language server

## Agent guide

flint lints, validates, migrates and generates Fleet GitOps YAML, and serves it to editors over LSP.

**Discovery:**
1. Pick a command from the index below
2. `flint help-ai --command <name>` — full flags (dotted for subcommands: `gen.software`)
3. `flint help-ai --sop <name>` — step-by-step procedure (list below)
4. `flint help-ai --full` — every command and flag (large)
5. `flint help-json [<name>]` — the CLI schema as JSON

**Common patterns:**
- `flint check .` — exit 0 = no errors, 1 = errors, 3 = config does not parse (101 = crash); `--detailed-exitcodes` → 2 on any finding
- `flint dry-run .` — whole-repo gate before `fleetctl gitops`; exit 2 = blocking findings
- Structured output: `check --format json|github|markdown`, `dry-run --json`, `history --json`, `list-rules --format json`
- `--fix` applies safe fixes; add `--unsafe-fixes` for semantic ones
- `flint migrate <path>` prints a JSON report — it does NOT change files
- Suppress one line: `# flint: ignore [rule-a, rule-b]` (rule ids from `flint list-rules`)
- Config: `fleetlint.toml` (legacy `.fleetlint.toml` also read) — rules, thresholds, files, Fleet connection; `flint init` writes one
- Repo root: nearest dir with a flint config, else nearest with `default.yml`; `.git` only as fallback

**SOPs (`flint help-ai --sop <name>`) — match the user's intent:**
- `lint`      — lint/validate/fix files, a fleet or a profile; CI; offline dry-run of a merge
- `migrate`   — upgrade Fleet versions: teams→fleets, queries→reports, renamed keys
- `lsp`       — editor setup (VS Code, Zed, Sublime, any LSP client)
- `hooks`     — pre-commit (advisory or strict) and pre-push hooks
- `author`    — write Fleet GitOps YAML correctly the first time
- `paths`     — fix broken `path:` refs; wire unwired profiles/scripts/software
- `history`   — replay rules over git history, mine guardrails, gate rule quality in CI
- `software`  — generate stanzas from .pkg/.mobileconfig/.sql etc.; update packages; FMA slugs
- `add-field` — extend flint with a new Fleet YAML field (working ON the flint repo)
"#;

pub(crate) fn generate_index(cmd: &clap::Command, writer: &mut impl Write) -> Result<()> {
    let name = cmd.get_name();
    let mut buf = String::with_capacity(4 * 1024);
    buf.push_str(&AGENT_GUIDE.replace("flint ", &format!("{name} ")));
    writeln!(buf)?;
    writeln!(buf, "## Command index")?;
    writeln!(buf)?;
    for sub in cmd.get_subcommands() {
        if sub.is_hide_set() || SKIP_SUBCOMMANDS.contains(&sub.get_name()) {
            continue;
        }
        let about = sub.get_about().map(|a| a.to_string()).unwrap_or_default();
        let aliases: Vec<&str> = sub.get_visible_aliases().collect();
        let alias = if aliases.is_empty() {
            String::new()
        } else {
            format!(" (alias `{}`)", aliases.join("`, `"))
        };
        writeln!(buf, "- `{name} {}`{alias} — {about}", sub.get_name())?;
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
  ASSERT path is a directory, a YAML file, or a profile (.mobileconfig / DDM .json)

  # Phase 1 — Root and config
  flint walks up from <path>: the nearest `fleetlint.toml` (or legacy
  `.fleetlint.toml`) marks the repo root; else the nearest directory holding
  `default.yml`; `.git` is only the boundary and fallback. Analysis is always
  repo-wide from that root, whatever <path> you pass.
  IF no repo config: `~/.config/flint/config.toml` applies, else defaults.
  IF the config does not parse:
    `check` / `dry-run` stop with exit 3 and the parse error (`--hook-mode`
    warns and lints with defaults). Fix the file; never delete it to get green.
  IF stderr says "`<key>` is not a setting" or "is not a rule code":
    that setting has no effect — fix the typo (warning shows file:line).
  IF you want a repo config:
    RUN: `flint init --no-interactive`        # writes fleetlint.toml, unnarrowed

  # Phase 2 — Run
  RUN: `flint check <path>`
  IF the question is about ONE fleet:
    RUN: `flint check <repo> --fleet <glob>`  # matches path, file name, stem or `name:`
    OR:  `flint check fleets/<fleet>.yml`     # a file or sub-directory scopes the same way
    Only in-scope findings are reported and set the exit code; the rest are
    counted (JSON: summary.scope.findings_outside_not_shown). A --fleet glob
    matching nothing is an error (exit 1), never "clean".
  IF path is a profile: `flint check <profile>` checks that one artifact.
  SEVERITY: flint reports the coupling it can see, not the failure it cannot
    verify — a finding in a fragment no fleet wires is a WARNING; the same
    finding in a wired fragment is an ERROR.

  # Phase 3 — Classify exit
  IF exit == 0:
    RETURN: no errors (warnings and infos never fail)
  IF exit == 1:
    errors found, or an engine error (e.g. path does not exist)
    FOR EACH error d:
      READ d.help (what is wrong) and d.suggestion (literal replacement, if any)
    IF the summary prints "N auto-fixable":
      RUN: `flint check <path> --fix`          # Safe fixes only
      "N more with --fix --unsafe-fixes":      review the diff after applying
    IF d is a confirmed false positive:
      (a) inline:    `# flint: ignore [<rule>]` at the end of the line, or alone on
                     the line directly above; `[a, b]` for several; no list = all rules.
                     A code no rule emits is reported (`ignore-comment-unknown-rule`).
      (b) repo-wide: `[rules] disabled = ["<rule>"]` or `warn = ["<rule>"]`
    HALT until errors == 0
  IF exit == 3:
    the config file does not parse — fix it (see Phase 1)
  IF exit == 101:
    flint panicked — file an issue with stderr

  # Phase 4 — Machine-readable mode (CI / agents)
  RUN: `flint check <path> --format json`
  PARSE: { version, files: [{ path, counts: {errors, warnings, infos},
           diagnostics: [{ rule, severity, message, line, column,
                           help?, suggestion?, context?, related? }] }],
           summary: { files_linted, errors, warnings, infos, scope?, staged? } }
  `--format github` = Actions annotations; `--format markdown` (+ `--git`) = PR comment.
  EXIT: 0 = no errors, 1 = errors, 3 = config does not parse, 101 = panic.
        `--detailed-exitcodes`: 0 = no findings, 2 = any finding (any severity).
        `--hook-mode`: 0 whatever the findings.

  # Phase 5 — Whole-repo gate before push / merge (offline, no server)
  RUN: `flint dry-run <repo>`                   # would `fleetctl gitops` likely pass?
  EXIT (dry-run): 0 = pass, 2 = blocking errors; `--strict` also blocks on warnings;
                  `--json` for the verdict as JSON
  RUN: `flint dry-run <repo> --against <REF>`   # lint the MERGE of HEAD and REF without
                                                # producing it; a textual conflict blocks
  IF a `.fleet-snapshot.json` is present:
    it gates server-side existence (installers, labels); stale → `--refresh-snapshot`
  A clean dry-run means flint found nothing — weaker than fleetctl accepting the apply.
  `--oracle <gitops-oracle>` (dev-only) audits blocking findings against Fleet's
  own parser; advisory, never changes the exit code.

  # Phase 6 — Related tools
  `flint list-rules [--format json]`  # every rule code, category, fixable
  `flint paths <repo> [--fix]`        # broken `path:` refs and where the file moved
  `flint migrate <repo>`              # deprecated keys/dirs report for an upgrade
  `flint hooks install [--strict]`    # pre-commit: `flint check --staged`

# Configuration (fleetlint.toml)
```toml
[rules]
disabled = ["secret-hygiene"]        # silence rules entirely
warn = ["interval-validation"]       # downgrade their errors to warnings

[thresholds]
min_interval = 60                    # query interval bounds (seconds)
max_interval = 86400

[files]
# include unset = everything not excluded. To narrow, use DIRECTORY globs
# ("fleets/**"), never extension globs: a non-empty include also scopes the
# cross-file rules, which report on scripts/profiles.
exclude = ["archive/**"]

[deprecations]
fleet_version = "latest"             # target Fleet version (default "latest")
future_names = false                 # true: warn on old names before they are required

[orphans]
allow = ["profiles/archive/**"]      # kept on purpose; silences orphaned-file only

[fleet.env]
# Names this repo supplies to fleetctl. `env-var-resolvable` warns on any
# `${VAR}` not set here, in the environment, or server-resolved ($FLEET_SECRET_*,
# $FLEET_VAR_*). fleetctl expands the RAW text, comments included: escape a
# literal as `\${VAR}` (`--fix` does this inside comments).
FLEET_BOOTSTRAP_TOKEN = "op://Vault/Item/field"
```

# Anti-patterns
- Reading "exit 0" as "no findings" — warnings never fail; use `--detailed-exitcodes`.
- Ignoring stderr — an unparseable config silently drops all scoping and rule settings.
- `[files] include = ["**/*.yml"]` — switches off the cross-file rules.
- `[files] exclude` to quiet an orphan — use `[orphans] allow`.
- Disabling a rule repo-wide for one false positive — suppress inline.
- Treating a clean dry-run as proof the apply will succeed.
"#;

const SOP_MIGRATE: &str = r#"# SOP: Fleet GitOps Migration

PROCEDURE migrate_repo(root, target):
  ASSERT root is the GitOps repo root (the dir holding default.yml) — `migrate`
         takes the path literally and reports paths relative to it
  ASSERT target is the Fleet version the repo is applied to ("4.85.0";
         "latest" = flint's pinned Fleet). A rename is reported only once that
         Fleet accepts the new name: never migrate past your server.
  NOTE: Fleet still accepts every old name (with a warning) — migration is never forced.

  # Phase 1 — Report (read-only)
  RUN: `flint migrate <root> --target-version <target>` -> JSON
  PARSE: { version, target_version,
    summary: { files_scanned, deprecations_found, directory_renames, file_renames,
               key_renames, safe_fixes, unsafe_fixes },
    directory_renames: [{ old, new, files_affected }],
    file_renames:      [{ old, new, set_name? }],  # repo-relative, any depth; `new` already
                                                   # includes the directory rename
    file_changes:      [{ path, move_to?, key_renames: [{ line, old_key, new_key, safety }] }] }
  IF summary.deprecations_found == 0: RETURN "already current for <target>"

  # Phase 2 — Conflicts (before any rename)
  RUN: `flint check <root>`
  IF error "'<old>' (deprecated) and '<new>' are both present":
    MERGE the old block's entries into <new> by hand, then delete <old>.
    Fleet rejects such a file, and a rename (incl. `--fix`) makes a duplicate key.

  # Phase 3 — Moves (`git mv` in a git repo, else `mv`)
  FOR EACH dr IN directory_renames: RUN `git mv <root>/<dr.old> <root>/<dr.new>`
  FOR EACH fr IN file_renames:
    RUN `git mv <root>/<fr.old, with dr applied> <root>/<fr.new>`
    IF fr.set_name: set the file's top-level `name:` to it IN THE SAME STEP
       (Fleet rejects `unassigned.yml` without `name: Unassigned`; flint: `reserved-fleet-name`)

  # Phase 4 — Key renames
  SET `fleetlint.toml` → `[deprecations] fleet_version = "<target>"`
      (`check --fix` renames up to this version; unset = latest)
  RUN: `flint check <root> --fix`     # applies every safe fix, renames included
  RUN: `flint check <root>`           # --fix prints pre-fix findings; re-run for truth
  FALLBACK (no --fix): per file at its final path (file_renames.new, else move_to,
    else path), apply key_renames by line DESC: "<old_key>:" → "<new_key>:" on that line.

  # Phase 5 — References broken by the moves
  RUN: `flint paths <root> --fix`
  IF any `?` remains: commit the moves, re-run (--fix then follows the recorded git rename)

  # Phase 6 — Verify
  RUN: `flint migrate <root> --target-version <target>` → ASSERT summary.deprecations_found == 0
  RUN: `flint dry-run <root>` → ASSERT exit 0 (2 = blocking errors)
  RETURN: one reviewed diff, ready for PR

# Anti-patterns
- A target above the server version — writes keys that Fleet rejects.
- Running migrate on a subdirectory — directory renames are only found at <root>.
- `mv no-team.yml unassigned.yml` without `name: Unassigned`.
- `--fix` while an old+new conflict exists — produces a duplicate key.
- Hand-grepping `path:` values — `flint paths --fix` resolves them relative to each referrer.

# Renames by the Fleet release that introduced the new name (the report is authoritative)
- 4.54.0  profile entries: `labels` → `labels_include_all`
- 4.82.0  `team_settings` → `settings`;  `queries` → `reports`
- 4.83.0  `no-team.yml` → `unassigned.yml` (+ `name: Unassigned`)
          controls: `macos_settings` → `apple_settings`, `custom_settings` → `configuration_profiles`,
          `macos_setup` → `setup_experience`, `macos_setup_assistant` → `apple_setup_assistant`,
          `enable_release_device_manually` → `apple_enable_release_device_manually`,
          `script` → `macos_script`, `manual_agent_install` → `macos_manual_agent_install`
- 4.84.0  directory `teams/` → `fleets/`
- 4.85.0  setup_experience: `enable_managed_local_account` → `enable_create_local_admin_account`
- 4.86.0  org_info: `org_logo_url` → `org_logo_url_dark_mode`,
          `org_logo_url_light_background` → `org_logo_url_light_mode`
"#;

const SOP_LSP: &str = r#"# SOP: Editor Setup (LSP)

Every editor runs the same server: `flint lsp` over stdio. Platforms: macOS
Apple Silicon, Linux x64/arm64 (no macOS Intel build).

PROCEDURE setup_editor(editor):
  ASSERT `flint --version` succeeds (Zed/Sublime can auto-download; VS Code bundles)

  CASE editor:
    vscode:
      INSTALL `flint-<v>.vsix` from the GitHub release:
              `code --install-extension flint-<v>.vsix`   (binary bundled)
      SETTINGS: flint.serverPath (else bundled, else PATH), flint.fleetVersion,
                flint.trace.server = verbose for protocol logs
      Attaches to YAML under default.yml, fleets/, teams/, lib/, platforms/, labels/
      (+ JSON under enrollment-profiles/, declaration-profiles/).
    zed:
      INSTALL as a dev extension: "zed: install dev extension" → `editors/zed`
              of a checkout of headmin/fleet-editor-extensions
      EDIT Zed settings: `"languages": {"YAML": {"language_servers": ["flint-lsp"]}}`
           (this list replaces yaml-language-server — it conflicts)
      Binary: PATH, then ~/.cargo/bin, /opt/homebrew/bin, /usr/local/bin, else download.
    sublime:
      INSTALL the `LSP` package (Package Control), then copy `editors/sublime`
              to Packages/LSP-flint. Binary: PATH, common dirs, else download.
    other (Neovim etc.):
      START `flint lsp` for yaml buffers, root at the dir holding default.yml
      (Neovim: `require('flint').setup()` from editors/neovim).

  # Verification
  OPEN the GitOps repo ROOT as the workspace (config is found by walking UP
       from it — a fleetlint.toml below the workspace root is not loaded)
  ASSERT hover on a known key shows docs; a typo'd key gets a diagnostic
  ASSERT `path:` completion lists workspace files; `$FLEET_VAR_` completes
  IF nothing appears: VS Code "Fleet: Show Output Channel" / "Fleet: Restart Language Server"

# Capabilities
- diagnostics      per-file on change (debounced), cross-file pass on save
- completion       keys, values, osquery tables, labels, FMA slugs, path globs,
                   `$FLEET_VAR_*` (triggers `: - / . $` and space)
- hover            field + osquery table docs
- code actions     quick fixes; "fix all N references" for a moved file
- definition/links `path:` references
- symbols, folding, semantic tokens
- `# flint: ignore [rule-a, rule-b]` suppresses inline, same as the CLI

# Configuration — same fleetlint.toml (or .fleetlint.toml) as `flint check`
- [rules] disabled/warn · [thresholds] · [files] include/exclude
- [deprecations] fleet_version, future_names (editor fleetVersion overrides)
- [lsp] lint_debounce_ms (default 150)
- No repo config → ~/.config/flint/config.toml
- Edits to `fleetlint.toml` (or `.fleetlint.toml`) reload live.

# Optional: server-backed features (both off by default; shell out to fleetctl)
  [fleet]
  gitops_validation = true   # on save: `fleetctl gitops --dry-run -f <default.yml>`
  live_completions  = true   # `fleetctl get labels|teams|queries --yaml`
  url / token                # else FLEET_URL / FLEET_API_TOKEN from the EDITOR's env
  fleetctl_path              # else `fleetctl` on PATH
  [fleet.env]                # vars the YAML references; `op://` values resolved
  Never commit `token`; GUI-launched editors may not see shell env vars.
"#;

const SOP_HOOKS: &str = r#"# SOP: Git hooks (pre-commit, pre-push)

PROCEDURE install_hook(mode):
  ASSERT cwd is inside the GitOps git repo (hooks go where git runs them:
         `core.hooksPath` if set, else <gitdir>/hooks)
  CASE mode:
    advisory (default): `flint hooks install`                   # never blocks
    strict:             `flint hooks install --strict`          # errors block the commit
    json:               add `--json` to either (flint emits --format=json)
    pre-push:           `flint hooks install --pre-push`        # blocking `flint dry-run .`
  ASSERT the output says "✓ Installed"; "already installed" means NOTHING
         changed — re-run with `--force` to switch modes.
  IF a hook not written by flint exists: install refuses; `--force` overwrites
     it, so move it aside first if it must survive.

PROCEDURE remove_hook():
  RUN `flint hooks uninstall`      # removes flint's pre-commit AND pre-push hooks
  A hook flint did not write is reported and left in place.

# What the pre-commit hook runs
- `flint check --staged [--format=json] [--hook-mode] .` once per commit.
- Analysis is repo-wide (cross-file rules run); only findings touching a
  staged file — directly or via a cross-file reference — are reported/block.
  Renames stay in scope (old path checked). Nothing staged → nothing reported.
- advisory: `--hook-mode` → always exit 0. strict: exits 1 when flint reports errors;
  warnings never block. Bypass once: `git commit --no-verify`.
- flint missing from PATH → hook prints a warning and exits 0.
- Inspect: `head -10 .git/hooks/pre-commit` → "Strict mode: <bool>. JSON output: <bool>."

# What the pre-push hook runs
- `flint dry-run .` over the whole repo; blocking findings abort the push
  (warnings advisory). Bypass once: `git push --no-verify`.

# Alternative: pre-commit framework (.pre-commit-config.yaml)
  - repo: https://github.com/headmin/fleet-editor-extensions
    rev: <release tag>
    hooks: [{id: flint}]          # `flint check .` on any Fleet YAML change
  `id: flint-files` passes the changed files instead; flint still roots the
  analysis at the repo. Both are blocking on errors and need flint on PATH.

# Practice
- Start advisory on a repo with existing findings; go strict once `flint check .` is clean.
- Pair strict pre-commit with `--pre-push` or CI `flint dry-run`.
"#;

const SOP_AUTHOR: &str = r#"# SOP: Authoring Fleet GitOps YAML against flint's schema

PROCEDURE author_fleet_yaml(target_file, intent):
  # Audience: any agent (or human) writing Fleet GitOps YAML in any repo.

  # Phase 1 — The path decides the file kind (flint auto-detects)
  CASE target_file:
    agent-options*.yml / agent_options*.yml   -> standalone agent_options
    any dir named labels/ , or *.labels.*     -> label array (top-level sequence)
    any dir named policies/                   -> policy array
    any dir named queries/ or reports/        -> query/report array
    any dir named software/                   -> software package config
    everything else (default.yml, fleets/*.yml, teams/*.yml) -> fleet/org config
  Files under scripts/, configuration-profiles/, declaration-profiles/, icons/,
  commands/ … are artifacts, not YAML to lint.

  # Phase 2 — Schema source
  `flint check` is the authority: its key schema is tested both ways against
  the keys Fleet's own parser accepts, so an unknown key is one `fleetctl
  gitops` rejects too. flint-lsp completes keys and `$FLEET_VAR_` names.
  Fleet's docs explain meaning; where they disagree with flint, flint is right.

  # Phase 3 — Draft
  START from `flint gen <software|policy|profile|query|fleet|label> [--from <file>]`
    rather than hand-writing a stanza (it computes hashes, platforms, identifiers).
  USE current names — old ones raise `deprecated-keys` with a suggestion, and
  old + new together in one file is an error:
    `fleets/` NOT `teams/`   `settings:` NOT `team_settings:`   `reports:` NOT `queries:`
    `controls.apple_settings` NOT `macos_settings`   (`flint migrate <repo>` lists all)
  `unassigned.yml` must say `name: Unassigned`; `no-team.yml` must say `name: No team`.
  `$FLEET_VAR_*`: each file type (Apple/Windows profiles, DDM, scripts) allows
    its own set — `fleet-variable` reports one Fleet would reject.
  `${VAR}` (env) is expanded by fleetctl in the RAW text, comments included:
    escape a literal as `\${VAR}`.
  `path:` / `paths:` resolve relative to the file that contains them.

  # Phase 4 — Validate
  RUN: `flint check <target_file>`    # analysis stays repo-wide; report is scoped
  IF exit == 1 (errors):
    APPLY `help` / `suggestion`, or `flint check <target_file> --fix`
    LOOP back to Phase 3
  IF warnings: fix them unless clearly intentional — exit 0 does not mean clean.
  IF a `path:` is broken: `flint paths <repo>` shows where the file went.

  # Phase 5 — Gate
  RUN: `flint dry-run <repo>`         # exit 2 = blocking errors
  Commit hook: `flint hooks install` (see `flint help-agents --sop hooks`).

# What flint catches that a plain YAML linter does not
- keys Fleet's parser rejects, or valid keys at the wrong level (org- vs fleet-only)
- `type: patch` policies without `fleet_maintained_app_slug`
- osquery tables unavailable on the policy's platform (`platform: darwin,linux` split)
- broken `path:` references, unwired/orphaned profiles and scripts
- duplicate fleet names (Fleet silently keeps one)
- `$FLEET_VAR_*` unknown or not allowed in that file type; unset `${VAR}`

# Anti-patterns
- Copying a key from Fleet's docs without running `flint check` — docs list keys
  the parser rejects (e.g. `require_all_software`).
- Renaming `no-team.yml` without changing its `name:` (or vice versa).
- Putting `${VAR}` in a comment as documentation.
"#;

const SOP_ADD_FIELD: &str = r#"# SOP: Add a new Fleet YAML field to flint
# Audience: agents working ON this repo (extending the schema/linter).

PROCEDURE add_fleet_field(field_name, parent_path, field_type, valid_values?):
  # parent_path uses dot notation:
  #   "controls"             "policies[]"           "labels[].criteria"
  #   "agent_options"        "software.packages[]"  "settings.webhook_settings"

  # Phase 1 — Source of truth: Fleet's parser, not its docs
  crates/flint-lint/data/fleet-gitops-keys.json is generated from Fleet's Go
  types at the version pinned in tools/gitops-oracle/go.mod: every GitOps key
  plus the $FLEET_VAR_ allow-lists.
  IF the field is new in a newer Fleet:
    RUN: `(cd tools/gitops-oracle && go mod edit -require=github.com/fleetdm/fleet/v4@<vX.Y.Z> && GOSUMDB=off go mod tidy)`
    RUN: `(cd tools/gitops-oracle && go run ./cmd/schema-extract) > crates/flint-lint/data/fleet-gitops-keys.json`
    EDIT crates/flint-lint/src/version_gate.rs `latest()` to the same version
         (test `latest_matches_the_key_snapshot` enforces it)
  RUN: `cargo test -p flint-lint --lib strict_schema`
    -> `strict_schema_accepts_every_fleet_gitops_key` lists Fleet keys flint lacks;
       `strict_schema_accepts_no_key_fleet_rejects` lists flint keys Fleet rejects.
  To settle one key: build the oracle (`cd tools/gitops-oracle && go build -o
  gitops-oracle .`), write a tiny YAML using it, run `./gitops-oracle <file>`.

  # Phase 2 — Structural schema
  EDIT crates/flint-lint/src/structure.rs
  LOCATE the helper that builds <parent_path>'s children
         (e.g. policy_inline_strict, controls_schema, org_integrations)
         — org- and fleet-level variants differ where Fleet's types differ
  ADD: ("<field_name>", <node>)
       where <node> is one of:
         leaf()                       — opaque scalar
         boolean_leaf()               — strict bool
         array(item_node)             — sequence
         mapping(vec![...])           — fixed-key object
         open_mapping()               — any keys accepted
  A key Fleet's Go types reach but authors never write goes in the test's
  NOT_AUTHORED list with a reason instead.

  # Phase 3 — Key registry (typo + misplaced-key detection)
  EDIT same file (KEY_REGISTRY block)
  ADD: reg.register("<field_name>", "<parent_path>");

  # Phase 4 — Editor surfaces (flint-lsp)
  EDIT crates/flint-lsp/src/schema.rs FIELD_DOCS — "<parent_path>.<field_name>" (hover)
  EDIT crates/flint-lsp/src/completion.rs — the matching `complete_*_fields`

  # Phase 5 — Semantics, only if the value has rules
  Enum values, cross-key coupling or a rename belong in a rule
  (crates/flint-lint/src/semantic.rs) or the DEPRECATION_REGISTRY
  (deprecations.rs), not in the schema node.

  # Phase 6 — Verify
  RUN: `cargo test --workspace`
  RUN: `cargo clippy --workspace --all-targets`   (no warnings)
  RUN: `./target/debug/flint check <fixture>` on a YAML using the field
  RUN: `scripts/ecosystem.sh --base main [--corpus <dir>]`
    -> per-rule +/- of base vs working tree on Fleet's own GitOps config (pinned)
       and any local corpus. Reports, never fails (except crashes). Expect only
       the intended change; never commit a report from a private corpus.

# Anti-patterns
- Trusting Fleet's docs over the parser (docs list `require_all_software`;
  the parser accepts only `require_all_software_macos` / `_windows`).
- Copying a field from a same-named struct at the wrong level (org vs fleet,
  or an API response type) — the reverse strict_schema test catches it.
- Hand-editing fleet-gitops-keys.json — regenerate it.
- Bumping the go.mod pin without regenerating the snapshot and `latest()`.
- Adding a field to FIELD_DOCS but not structure.rs (hover without validation),
  or the reverse (validation without completion/hover).
"#;

const SOP_PATHS: &str = r#"# SOP: Path references — fix broken refs & wire unwired artifacts

Two inverse problems over the repo's reference graph:
  A. broken — a `path:` (or policy `install_software.package_path:`) whose target
     is missing, a directory, empty, or differs only in letter case
  B. unwired — an artifact on disk that no fleet config references
`flint paths` always exits 0 (it reports). Gate on `flint check` / `flint dry-run`.

PROCEDURE fix_broken_paths(dir):            # dir: repo dir or a single file
  # Phase 1 — Report
  RUN: `flint paths <dir>`
  READS per referring file:
    L<line>  - <old path>
             + <new path>                       # unique match on disk → --fix rewrites it
             ↳ renamed in <sha> "<subject>"     # the fix came from a git-recorded rename
    L<line>  ? <old path>  (<why>)              # --fix cannot help:
             ↳ deleted in <sha> "<subject>"     #   restore the file or drop the reference
             ↳ renamed to <new> in <sha> "…"    #   destination gone too: find it by hand
  Same findings as `flint check` rules path-exists / path-is-file / path-case /
  path-empty. Severity: error in a wired file, warning in a fragment no fleet wires.

  # Phase 2 — Apply
  RUN: `flint paths <dir> --fix`
  EFFECT: rewrites only the `+` references, relative to the referring file; touches
  no other fix. Rename chains are followed up to 5 hops (old → mid → new).
  IF a move is uncommitted and the basename is ambiguous: commit the move, re-run.
  RUN: `flint paths <dir>` → ASSERT "No broken path references found."
  Editor: quick-fix per candidate, plus "Fix all N references to '<file>' (moved file)".

PROCEDURE wire_unwired(root):
  ASSERT root holds default.yml — --unwired reads configs only from
         <root>/default.yml, <root>/fleets/** and <root>/teams/**
  IF fleet files still use `macos_settings`/`custom_settings`: run `--sop migrate` first

  # Phase 1 — Report
  RUN: `flint paths <root> --unwired`               # grouped YAML blocks to paste
  OR:  `flint paths <root> --unwired --oneline`     # `rel<TAB>section<TAB>wire`, one per artifact
  ROUTING (current Fleet keys):
    .mobileconfig, declaration-profiles/*.json → controls.apple_settings.configuration_profiles
    .xml                                       → controls.windows_settings.configuration_profiles
    .sh / .ps1                                 → controls.scripts
    enrollment-profiles/*.json                 → controls.setup_experience.apple_setup_assistant (one per fleet)
    software/*.yml                             → software.packages
    policies/*.yml                             → policies
    queries|reports/*.yml                      → reports
  Wire values are relative to where this repo's fleet files live.

  # Phase 2 — Wire
  AGENT: `flint paths <root> --unwired --prompt --only <fleet-file>`
    → per artifact: EDIT / UNDER / INSERT + two DONE-WHEN commands. Without --only
      the target file is a choice flint will not make for you.
    FOR EACH task: insert, THEN RUN both DONE-WHEN commands; ASSERT both hold.
  HUMAN: `flint paths <root> --unwired --interactive [--only <glob>] [--label-stubs[=blank|comment]]`
    per fleet: [y]es / [a]ll-remaining / [n]o / [s]kip group / [q]uit
    [y] → [g]lob (one `paths:` rule, no labels) or [p]er-file (labels_include_any +
          labels_exclude_any, independent). Writes only on an explicit yes; keeps the
          file's existing (old or new) key spelling.

  # Phase 3 — Verify
  RUN: `flint check <root>` → ASSERT no new errors
  RUN: `flint paths <root> --unwired --oneline` → ASSERT the wired artifacts are gone

# Key facts
- `path:` is one file; `paths:` is a glob (`*`, `?`, `**`) and cannot carry labels —
  per-file labels need the per-file style.
- Kept on purpose (archive, template)? `fleetlint.toml` `[orphans] allow = ["profiles/archive/**"]`
  silences the `orphaned-file` check rule only; `--unwired` still lists the file.
- Inserts go after the section's last real entry (never under trailing commented
  blocks) and turn an inline `key: []` into a block first.
- git is used only for archaeology (`↳` lines); everything else works without it.

# Anti-patterns
- Using `flint paths` exit status as a gate — it is always 0.
- Hand-editing `../` depths — `--fix` and `--prompt` compute them per referrer.
- Wiring an archive to silence a warning — declare it in `[orphans] allow` instead.
"#;

const SOP_SOFTWARE: &str = r#"# SOP: Generate Fleet GitOps stanzas from real artifacts

Read the real file; never hand-copy hashes, identifiers or UUIDs.
`--from` picks the generator by extension; stdout is copy-pasteable, `-o <file>`
appends. `--wire` prompts for the fleet file to insert into — run it from inside
the GitOps repo. After any generate/insert: `flint check .`

PROCEDURE add_software_package(installer):
  `flint gen software --from <file.pkg>`         # comment header + hash_sha256
  `flint gen software --from <file.pkg> --full`  # + url placeholder, self_service,
                                                 #   commented labels/categories/scripts
  `--setup-experience` → setup_experience: true (install during Setup Assistant);
    use it with --full, --standalone or --wire (the plain block ignores it)
  `--wire` → inserts the full stanza under software.packages; asks about setup_experience
  STANDALONE file (referenced via `- path:`), .pkg only, never overwrites:
    `--standalone`              → <name>.package.yml next to the .pkg (-2/-3 on collision)
    `--standalone -o <dir>`     → same name, in <dir>
    `--standalone -o <x>.yml`   → exactly that file (refuses if it exists)
  REPLACE the `https://REPLACE-ME.example.com/...` url — flint warns until you do.
  OTHER formats: .deb .ipa .msi .rpm read identifier/version when the host tool
    exists; .exe .tar.gz get hash + placeholders. .dmg is rejected — repackage to .pkg.
  BATCH: `--from <dir>` handles every .pkg in it (not with --wire).

PROCEDURE update_package(new_pkg, file):
  `flint gen software --from <new.pkg> --update <file>`
  Matches by package identifier; rewrites hash_sha256, header version, url filename,
  and the old hash in every other YAML file of the repo (policy install_software).

PROCEDURE install_check_policy(pkg):
  `flint gen policy --from <file.pkg>`                 # package_receipts, version_compare >=
  `... --apps`            # apps table by the installed .app's bundle id (from the pkg)
  `... --outdated-only`   # fails only when an older copy is installed
  `... --file <abs path>` # existence of a file on disk
  `... --enforce`         # + install_software.hash_sha256 → auto-install on failing hosts.
     List the same package under that fleet's software (the hint goes to stderr).

PROCEDURE default_scripts(pkg):
  `flint gen scripts --from <file.pkg> -o <dir>`   # Fleet's default install.sh + uninstall.sh
  Keep `$PACKAGE_ID` — Fleet substitutes it at upload.

PROCEDURE add_profile(file):
  `flint gen profile --from <file>`   # .mobileconfig · DDM .json · Windows CSP .xml
                                      # · *.dep.json → controls.setup_experience.apple_setup_assistant
  `path:` is relative to the CURRENT directory — run from the fleet file's dir or use --wire
  `--full` adds commented label stubs; `--from <dir>` batches.
  Duplicate/placeholder PayloadUUID: `--regen-uuid` rewrites the TOP-LEVEL UUID in place.
  If `contour` is installed, Apple-schema findings print on stderr.

PROCEDURE add_query_or_policy(sql):
  `flint gen query --from <file.sql>` / `flint gen policy --from <file.sql>`
  Platform = intersection of the osquery tables used; unknown table → commented placeholder.

PROCEDURE blank_template(kind):
  `flint gen <policy|query|label|fleet>` → YAML stanza/file
  `flint gen profile -o <name>.mobileconfig` → new .mobileconfig with a fresh PayloadUUID

# Fleet-maintained apps (no file): `flint fma search <name>` → paste the slug
  software.fleet_maintained_apps: - slug: <app>/<platform>   (e.g. santa/darwin)
  Optional: self_service, setup_experience, labels_*, categories, scripts, version.
  `flint fleet fma` checks the slug against what your Fleet server offers.
"#;

const SOP_HISTORY: &str = r#"# SOP: Rule history — replay, mine, diff, gate

Four read-only questions over a repo's first-parent git history. Each commit is
extracted with `git archive` into a scratch dir; the working copy is never touched.
  A. replay — which rules would have fired on past commits, and how often?
  B. mine   — which conventions were repaired by hand, repeatedly, with no rule?
  C. diff   — where does flint's blocking verdict disagree with Fleet's parser?
  D. gate   — has rule quality regressed since a stored baseline? (CI)

PROCEDURE setup(repo):
  ASSERT repo is inside a git repository; [PATH] only locates it — the WHOLE tree
         at the git root is replayed
  SCOPE: `fleetlint.toml` (or `.fleetlint.toml`) at the GIT ROOT is copied into every
         tree, so today's scope holds for all of history. None there → each tree uses
         its own committed config. `--scope-as-committed` forces that ("what would flint
         have said then"). `[files] include` globs also let unchanged commits be reused.
  RANGE: `--max N` (default 200) newest commits, or `--since <REF>` (REF exclusive..HEAD),
         which is ALSO capped to the newest --max — raise --max for a long range.

PROCEDURE replay(repo):
  RUN: `flint history <repo> [--max N | --since <REF>] --json`
  PARSE: { mode: "replay", scope, commits_replayed, commits_reused_unchanged_in_scope,
           commits_with_findings, not_replayed: [{code, reason}],
           codes: [{ code, closed_windows, still_firing_at_head, commits_affected,
                     windows: [{ opened_short, opened_subject, closed_short?,
                                 closed_subject?, commits }] }],
           oracle_diff }                            # null without --oracle
  A red window is a run of commits in which a code fired (errors AND warnings); it
  closes at the commit that made it stop. Rank by closed_windows: >= 2 = repeat failure.
  An open window (still_firing_at_head) is current state — fix it with `--sop lint`.
  not_replayed: software-source (needs server state at that commit, which is gone).

PROCEDURE suggest_patterns(repo):
  RUN: `flint history <repo> --suggest-patterns [--min-occurrences 2] [--json]`
  JSON: { candidates[{ id, key, files, assert, occurrences, evidence, rationale,
          pattern_toml }], forbid_candidates, wiring_candidates, hotspots, cannot_express }
  Heuristic, printed commented out, never written to config.
  FOR EACH candidate:
    IF a rule already covers it (e.g. a key rename = deprecated-keys): DISCARD
    ELSE paste pattern_toml into `fleetlint.toml`, RUN `flint check <repo>`,
         ASSERT it fires where intended and nowhere else.
  hotspots = evidence of where a real rule would pay off, not a pattern.

PROCEDURE oracle_diff(repo, oracle):   # dev/CI only — the oracle is not shipped
  BUILD (flint source tree): `cd tools/gitops-oracle && go build -o gitops-oracle .`
  RUN: `flint history <repo> --oracle <oracle>`
  Each tree also goes through Fleet's own parser (what `fleetctl gitops` runs), and
  blocking claims (errors only) are diffed both ways:
    flint blocked, Fleet accepted — per code:
      "expected — <why>"  enforced outside the parser: profile-well-formed,
                          duplicate-fleet-name, software-source
      "REVIEW — possible false positive"  narrow or fix the rule
    Fleet blocked, flint silent — the measured gap list
  Single tree (a PR): `flint dry-run <repo> --oracle <oracle>`.

PROCEDURE gate(repo, baseline):
  RUN: `flint history <repo> --since <FIXED_REF> --max <N> [--oracle <oracle>] --gate <baseline.json>`
  ASSERT range and --oracle match the run that wrote the baseline: a sliding --max
         window drops old closed windows and fails the gate for no rule change.
  IF baseline missing: it is written; PASS.
  EXIT 2 (FAIL) IF any of:
    - a code newly blocks where Fleet accepts            (needs --oracle)
    - Fleet newly blocks something flint is silent on    (needs --oracle)
    - a code's closed_windows fell below the baseline    (rule stopped catching)
  Rising counts and new rules are notes, never failures.
  Baseline recorded with --oracle, run without → oracle halves skipped (a note says so).
  With --json the replay doc is followed by a second doc { gate: pass|fail,
    regressions, improvements, notes, baseline_commits, commits }.
  IF every regression is understood and intended:
    RUN the same command with `--update-baseline`; COMMIT it with the rule change.
  NEVER update the baseline to silence a regression you have not explained.

# Key facts
- `--no-reuse` recomputes every commit; reuse is output-identical — use it only to
  prove that or to bisect a difference.
- Comparing two flint builds across real repos is `scripts/ecosystem.sh` (flint
  source tree; `--base <ref>`, `--corpus <dir>`): it reports rule diffs, fails only on crashes.
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
/// Creates `.claude/skills/flint/SKILL.md` and
/// `.claude/skills/fleet-migrate/SKILL.md` — Claude Code loads a skill from a
/// directory holding `SKILL.md`, not from a flat `.md` file — then ensures
/// `CLAUDE.md` has a bootstrap hint.
pub(crate) fn install_skill(version: &str) -> Result<()> {
    install_skill_in(Path::new("."), version)
}

fn install_skill_in(root: &Path, version: &str) -> Result<()> {
    use std::fs;

    // 1. Install skill files
    for (name, template) in &[("flint", SKILL_FLINT), ("fleet-migrate", SKILL_FLEET_MIGRATE)] {
        let dir = root.join(".claude/skills").join(name);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("SKILL.md"), template.replace("{{VERSION}}", version))?;
        eprintln!("\u{2713} Installed Claude Code skill: .claude/skills/{name}/SKILL.md");
        // Earlier flint versions wrote flat files Claude Code never loads.
        let flat = root.join(format!(".claude/skills/{name}.md"));
        if flat.is_file() {
            eprintln!("  .claude/skills/{name}.md is from an older flint and is not loaded — you can delete it");
        }
    }

    // 2. Ensure CLAUDE.md has a flint bootstrap hint
    let bootstrap_line = "## flint\n\n\
        `flint` (Fleet GitOps linter) is available. \
        Run `flint setup-agent` to install the AI agent skill, \
        or `flint help-ai` for the command reference.\n";

    let claude_md = root.join("CLAUDE.md");
    let claude_md = claude_md.as_path();
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
    fn setup_agent_writes_loadable_skill_dirs() {
        let dir = tempfile::tempdir().unwrap();
        install_skill_in(dir.path(), "9.9.9").unwrap();
        for name in ["flint", "fleet-migrate"] {
            let body = std::fs::read_to_string(dir.path().join(format!(".claude/skills/{name}/SKILL.md"))).unwrap();
            assert!(body.starts_with("---\nname: "), "frontmatter first: {name}");
            assert!(body.contains(&format!("name: {name}\n")), "{name}");
            assert!(body.contains("9.9.9"));
        }
        assert!(dir.path().join("CLAUDE.md").is_file());
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
