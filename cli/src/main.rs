//! Flint CLI — Fleet GitOps YAML linter and language server.
//!
//! Thin command dispatcher that delegates to `flint-lint` (linting engine)
//! and `flint-lsp` (language server). See `flint help-ai` for agent discovery.

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use flint_lint as linter;
use flint_lsp as lsp;

#[derive(Parser)]
#[command(name = "flint")]
#[command(version = concat!(
    env!("CARGO_PKG_VERSION"), "+", env!("BUILD_TIMESTAMP"),
    " (Fleet sync: ", env!("FLEET_SYNC_COMMIT"), ", ", env!("FLEET_SYNC_DATE"), ")"
))]
#[command(about = "Flint — Fleet GitOps YAML linter and language server", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check (lint) YAML file(s) with Fleet-specific validation
    #[command(alias = "lint")]
    Check {
        /// File or directory to lint
        path: PathBuf,

        /// Automatically apply safe fixes
        #[arg(long)]
        fix: bool,

        /// Also apply fixes that may change semantics (requires --fix)
        #[arg(long)]
        unsafe_fixes: bool,

        /// Output format
        #[arg(short, long, default_value = "text", value_parser = ["text", "json", "markdown"])]
        format: String,

        /// Run as a non-blocking git hook: print diagnostics but always exit 0,
        /// so warnings/errors don't block commits. Suitable for `.git/hooks/pre-commit`.
        #[arg(long)]
        hook_mode: bool,

        /// Use terraform-style exit codes: 0 = no findings, 1 = engine error,
        /// 2 = findings detected (any severity). Without this flag, only
        /// errors trigger a non-zero exit. Ignored when --hook-mode is set.
        #[arg(long)]
        detailed_exitcodes: bool,

        /// CI mode: auto-post the markdown report as a PR comment via `gh`.
        /// Currently supports GitHub Actions (detected via $GITHUB_ACTIONS
        /// and $GITHUB_REF). Implies --format markdown; errors if --format
        /// is set to something else. Requires `gh` on PATH and a token with
        /// PR-comment scope. On post failure, the body still prints to
        /// stdout so the CI logs preserve it.
        #[arg(long)]
        git: bool,

        /// Override the markdown heading. Useful when a monorepo PR posts
        /// multiple flint reports (e.g. one per sub-project) and readers
        /// need to tell them apart. Only affects `--format markdown`.
        #[arg(long, value_name = "TEXT")]
        heading: Option<String>,
    },

    /// Manage git hooks for non-blocking flint validation in a Fleet GitOps repo.
    Hooks {
        #[command(subcommand)]
        action: HooksAction,
    },

    /// Start language server (called by editor extensions, not directly)
    #[command(hide = true)]
    Lsp {
        /// Enable debug logging to stderr
        #[arg(long)]
        debug: bool,

        /// Use stdio transport (default, accepted for compatibility)
        #[arg(long)]
        stdio: bool,
    },

    /// Initialize Fleet linter configuration
    ///
    /// Creates a .fleetlint.toml configuration file in the current directory.
    /// Auto-detects your Fleet GitOps structure and generates sensible defaults.
    Init {
        /// Output path for config file (default: .fleetlint.toml)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Skip interactive prompts, use detected/default values
        #[arg(long)]
        no_interactive: bool,

        /// Force overwrite existing config
        #[arg(short, long)]
        force: bool,
    },

    /// List all available lint rules
    ListRules {
        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Generate a migration report for upgrading Fleet GitOps YAML
    Migrate {
        /// Root directory of the GitOps repo
        path: PathBuf,

        /// Target Fleet version (e.g., "4.85.0" or "latest")
        #[arg(long, default_value = "latest")]
        target_version: String,
    },

    /// Output CLI reference for AI agents (default: command index)
    #[command(name = "help-agents", alias = "help-ai")]
    HelpAgents {
        /// Show full detail for a specific command (dot notation)
        #[arg(long)]
        command: Option<String>,

        /// Show standard operating procedures for a tool (lint, migrate, lsp)
        #[arg(long)]
        sop: Option<String>,

        /// Output the complete reference (all commands, all flags)
        #[arg(long)]
        full: bool,

        /// Install Claude Code skill files for flint
        #[arg(long)]
        install_skill: bool,
    },

    /// Install AI agent skill files (.claude/skills/)
    #[command(name = "setup-agent")]
    SetupAgent,

    /// Output CLI schema as JSON for tooling integration
    #[command(name = "help-json", hide = true)]
    HelpJson {
        /// Command path to scope output (dot notation, e.g. check)
        command: Option<String>,
    },

    /// Show the directory tree of a Fleet GitOps repo
    Tree {
        /// Root directory (default: current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Print detailed build information (version, build date, target triple,
    /// Fleet sync info). For a single-line version string, use `flint --version`.
    Version,
}

#[derive(Subcommand)]
enum HooksAction {
    /// Install a pre-commit hook in the current git repo.
    /// The hook runs `flint check` against staged YAML files (or the whole
    /// repo if none are staged) and prints diagnostics. By default the hook
    /// is non-blocking (warnings only); use --strict to block commits on
    /// errors.
    Install {
        /// Overwrite an existing hook without prompting
        #[arg(short, long)]
        force: bool,

        /// Strict mode: errors block the commit. Without --strict the hook
        /// is informational only and always allows the commit.
        #[arg(long)]
        strict: bool,

        /// Emit JSON diagnostics from flint instead of human-readable text.
        /// Useful for piping into other tools or CI integrations.
        #[arg(long)]
        json: bool,
    },
    /// Remove flint's pre-commit hook from the current git repo.
    Uninstall,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Check {
            path,
            fix,
            unsafe_fixes,
            mut format,
            hook_mode,
            detailed_exitcodes,
            git,
            heading,
        } => {
            use linter::Linter;

            use colored::Colorize;

            // --git implies --format markdown. Reject the contradictory case
            // (--git --format json) early so CI fails fast instead of
            // posting a comment in the wrong format or skipping silently.
            format = resolve_format_for_git(git, &format)?;

            // Use `from_path` so any `.fleetlint.toml` discovered in or above
            // the target path is loaded — `Linter::new()` skips it (issue #5).
            let linter = Linter::from_path(&path);
            let json_mode = format == "json";
            let markdown_mode = format == "markdown";
            let structured_mode = json_mode || markdown_mode;

            let mut total_errors = 0;
            let mut total_warnings = 0;
            let mut total_infos = 0;
            let files_linted;
            let mut markdown_body: Option<String> = None;

            if path.is_file() {
                let source = std::fs::read_to_string(&path)?;
                let report = linter.lint_file(&path)?;

                // Apply fixes if requested
                if fix {
                    let fixed = apply_fixes(&path, &report, unsafe_fixes)?;
                    if fixed > 0 && !structured_mode {
                        println!(
                            "{} Fixed {} issue(s) in {}",
                            "✓".green(),
                            fixed,
                            path.display()
                        );
                    }
                }

                files_linted = 1;
                total_errors = report.errors.len();
                total_warnings = report.warnings.len();
                total_infos = report.infos.len();

                if json_mode {
                    let output = lint_report_to_json(&path.display().to_string(), &report);
                    let wrapper = serde_json::json!({
                        "version": env!("CARGO_PKG_VERSION"),
                        "files": [output],
                        "summary": {
                            "files_linted": 1,
                            "errors": total_errors,
                            "warnings": total_warnings,
                            "infos": total_infos,
                        }
                    });
                    println!("{}", serde_json::to_string_pretty(&wrapper)?);
                } else if markdown_mode {
                    let body = render_markdown_report(
                        &[(path.display().to_string(), &report)],
                        files_linted,
                        total_errors,
                        total_warnings,
                        total_infos,
                        heading.as_deref(),
                    );
                    print!("{}", body);
                    markdown_body = Some(body);
                } else {
                    println!("{} Linting {}...\n", "🔍".blue(), path.display());
                    report.print(Some(&source));
                }
            } else if path.is_dir() {
                let results = linter.lint_directory(&path, None)?;

                // Apply fixes if requested
                if fix {
                    let mut total_fixed = 0;
                    for (file_path, report) in &results {
                        if let Ok(n) = apply_fixes(file_path, report, unsafe_fixes) {
                            total_fixed += n;
                        }
                    }
                    if total_fixed > 0 && !structured_mode {
                        println!("{} Fixed {} issue(s)\n", "✓".green(), total_fixed);
                    }
                }

                files_linted = results.len();

                if json_mode {
                    let mut file_outputs = Vec::new();

                    for (file_path, report) in &results {
                        total_errors += report.errors.len();
                        total_warnings += report.warnings.len();
                        total_infos += report.infos.len();
                        file_outputs.push(lint_report_to_json(
                            &file_path.display().to_string(),
                            report,
                        ));
                    }

                    let wrapper = serde_json::json!({
                        "version": env!("CARGO_PKG_VERSION"),
                        "files": file_outputs,
                        "summary": {
                            "files_linted": files_linted,
                            "errors": total_errors,
                            "warnings": total_warnings,
                            "infos": total_infos,
                        }
                    });
                    println!("{}", serde_json::to_string_pretty(&wrapper)?);
                } else if markdown_mode {
                    let pairs: Vec<(String, &linter::error::LintReport)> = results
                        .iter()
                        .map(|(p, r)| (p.display().to_string(), r))
                        .collect();
                    for (_, report) in &pairs {
                        total_errors += report.errors.len();
                        total_warnings += report.warnings.len();
                        total_infos += report.infos.len();
                    }
                    let body = render_markdown_report(
                        &pairs,
                        files_linted,
                        total_errors,
                        total_warnings,
                        total_infos,
                        heading.as_deref(),
                    );
                    print!("{}", body);
                    markdown_body = Some(body);
                } else {
                    println!("{} Linting directory {}...\n", "🔍".blue(), path.display());

                    for (file_path, report) in &results {
                        if report.total_issues() > 0 {
                            println!("\n{} {}", "File:".bold(), file_path.display());

                            if let Ok(source) = std::fs::read_to_string(file_path) {
                                report.print(Some(&source));
                            } else {
                                report.print(None);
                            }

                            total_errors += report.errors.len();
                            total_warnings += report.warnings.len();
                            total_infos += report.infos.len();
                        }
                    }

                    println!("\n{}", "=".repeat(60));
                    println!("{} Linted {} file(s)", "Summary:".bold(), files_linted);
                    println!("  {} error(s)", total_errors.to_string().red());
                    println!("  {} warning(s)", total_warnings.to_string().yellow());
                    println!("  {} info", total_infos.to_string().blue());
                }
            } else {
                anyhow::bail!("Path does not exist: {}", path.display());
            }

            // Post BEFORE applying the exit code — a non-zero exit
            // shouldn't skip surfacing findings to the PR.
            if git {
                if let Some(body) = &markdown_body {
                    if let Err(e) = post_pr_comment(body) {
                        eprintln!("flint: --git post skipped: {}", e);
                    }
                }
            }

            if !hook_mode {
                let total_findings = total_errors + total_warnings + total_infos;
                let code = resolve_exit_code(detailed_exitcodes, total_errors, total_findings);
                if code != 0 {
                    std::process::exit(code);
                }
            }
        }

        Commands::Hooks { action } => match action {
            HooksAction::Install {
                force,
                strict,
                json,
            } => install_pre_commit_hook(force, strict, json)?,
            HooksAction::Uninstall => uninstall_pre_commit_hook()?,
        },

        Commands::Lsp { debug, stdio: _ } => {
            // Set up logging if debug mode is enabled
            if debug {
                eprintln!("Fleet LSP server starting in debug mode...");
                // TODO: Set up tracing/logging to stderr
            }

            // Start the LSP server - this blocks until the client disconnects
            // Note: stdio transport is always used, the --stdio flag is accepted for compatibility
            lsp::start_server().await?;
        }

        Commands::Init {
            output,
            no_interactive,
            force,
        } => {
            let current_dir = std::env::current_dir()?;
            linter::init_config(&current_dir, output, !no_interactive, force)?;
        }

        Commands::ListRules { format } => {
            let ruleset = linter::RuleSet::default_rules();

            if format == "json" {
                let rules: Vec<serde_json::Value> = ruleset
                    .rules()
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "name": r.name(),
                            "description": r.description(),
                            "category": r.category(),
                            "fixable": r.is_fixable(),
                            "preview": r.is_preview(),
                            "severity": format!("{:?}", r.default_severity()),
                            "docs_url": r.docs_url(),
                        })
                    })
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "rules": rules,
                        "total": rules.len(),
                    }))?
                );
            } else {
                use colored::Colorize;
                println!("{}", "Fleet GitOps Lint Rules".bold());
                println!("{}", "=".repeat(90));
                println!(
                    "{:<28} {:<14} {:<8} {}",
                    "Rule".bold(),
                    "Category".bold(),
                    "Fixable".bold(),
                    "Description".bold()
                );
                println!("{}", "-".repeat(90));
                for rule in ruleset.rules() {
                    let fixable = if rule.is_fixable() {
                        "yes".green()
                    } else {
                        "no".dimmed()
                    };
                    println!(
                        "{:<28} {:<14} {:<8} {}",
                        rule.name(),
                        rule.category(),
                        fixable,
                        rule.description()
                    );
                }
                println!("{}", "-".repeat(90));
                println!("{} rule(s) total", ruleset.rules().len());
            }
        }

        Commands::Migrate {
            path,
            target_version,
        } => {
            use linter::{
                DeprecationKind, FixSafety, Linter, RuleSet, VersionContext, DEPRECATION_REGISTRY,
            };

            if !path.is_dir() {
                anyhow::bail!("Not a directory: {}", path.display());
            }

            // Build version context with future_names enabled so all active deprecations fire
            let mut version_ctx = VersionContext::from_config(&target_version);
            version_ctx.future_names = true;

            let target_ver = version_ctx.version.clone();
            let linter = Linter::with_rules(RuleSet::default_rules_with_version(version_ctx));
            let results = linter.lint_directory(&path, None)?;

            // Collect deprecation diagnostics from lint results
            let mut file_changes: Vec<serde_json::Value> = Vec::new();
            let mut total_key_renames = 0usize;
            let mut total_safe = 0usize;
            let mut total_unsafe = 0usize;

            for (file_path_buf, report) in &results {
                let all_errors: Vec<&linter::LintError> = report
                    .errors
                    .iter()
                    .chain(report.warnings.iter())
                    .chain(report.infos.iter())
                    .collect();

                let key_renames: Vec<serde_json::Value> = all_errors
                    .iter()
                    .filter(|e| e.rule_code.as_deref() == Some("deprecated-keys"))
                    .filter(|e| e.context.is_some() && e.suggestion.is_some())
                    .map(|e| {
                        let safety = match e.fix_safety.as_ref() {
                            Some(FixSafety::Safe) => "safe",
                            _ => "unsafe",
                        };
                        if safety == "safe" {
                            total_safe += 1;
                        } else {
                            total_unsafe += 1;
                        }
                        serde_json::json!({
                            "line": e.line.unwrap_or(0),
                            "old_key": e.context.as_deref().unwrap_or(""),
                            "new_key": e.suggestion.as_deref().unwrap_or(""),
                            "safety": safety,
                        })
                    })
                    .collect();

                if key_renames.is_empty() {
                    continue;
                }

                total_key_renames += key_renames.len();

                // Compute relative path and potential move_to
                let file_path_str = file_path_buf.display().to_string();
                let rel_path = file_path_str
                    .strip_prefix(&format!("{}/", path.display()))
                    .or_else(|| file_path_str.strip_prefix(&path.display().to_string()))
                    .unwrap_or(&file_path_str);

                let mut entry = serde_json::json!({
                    "path": rel_path,
                    "key_renames": key_renames,
                });

                // Check if this file is inside a directory that needs renaming
                for dep in DEPRECATION_REGISTRY.active_directory_renames(&target_ver) {
                    if let DeprecationKind::DirectoryRename { old_dir, new_dir } = &dep.kind {
                        let prefix = format!("{}/", old_dir);
                        if rel_path.starts_with(&prefix) {
                            let new_path = format!("{}/{}", new_dir, &rel_path[prefix.len()..]);
                            entry
                                .as_object_mut()
                                .unwrap()
                                .insert("move_to".into(), serde_json::json!(new_path));
                        }
                    }
                }

                file_changes.push(entry);
            }

            // Scan for directory renames
            let mut dir_renames: Vec<serde_json::Value> = Vec::new();
            for dep in DEPRECATION_REGISTRY.active_directory_renames(&target_ver) {
                if let DeprecationKind::DirectoryRename { old_dir, new_dir } = &dep.kind {
                    let old_path = path.join(old_dir);
                    if old_path.is_dir() {
                        let file_count = walkdir_count(&old_path);
                        dir_renames.push(serde_json::json!({
                            "old": old_dir,
                            "new": new_dir,
                            "files_affected": file_count,
                        }));
                    }
                }
            }

            // Scan for file renames from registry
            let mut file_renames: Vec<serde_json::Value> = Vec::new();
            for dep in DEPRECATION_REGISTRY.active_file_renames(&target_ver) {
                if let DeprecationKind::FileRename { old_name, new_name } = &dep.kind {
                    if path.join(old_name).exists() {
                        file_renames.push(serde_json::json!({
                            "old": old_name,
                            "new": new_name,
                        }));
                    }
                }
            }

            let report = serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "target_version": target_ver.to_string(),
                "summary": {
                    "files_scanned": results.len(),
                    "deprecations_found": total_key_renames + dir_renames.len() + file_renames.len(),
                    "directory_renames": dir_renames.len(),
                    "file_renames": file_renames.len(),
                    "key_renames": total_key_renames,
                    "safe_fixes": total_safe,
                    "unsafe_fixes": total_unsafe,
                },
                "directory_renames": dir_renames,
                "file_renames": file_renames,
                "file_changes": file_changes,
            });

            println!("{}", serde_json::to_string_pretty(&report)?);
        }

        Commands::SetupAgent => {
            linter::help_agents::install_skill(env!("CARGO_PKG_VERSION"))?;
        }

        Commands::HelpAgents {
            command,
            sop,
            full,
            install_skill,
        } => {
            if install_skill {
                linter::help_agents::install_skill(env!("CARGO_PKG_VERSION"))?;
                return Ok(());
            }
            use clap::CommandFactory;
            let cmd = Cli::command();
            let mut out = std::io::stdout();
            if let Some(tool) = sop {
                linter::help_agents::generate_sop(&tool, &mut out)?;
            } else if let Some(path) = command {
                linter::help_agents::generate_command(&cmd, &path, &mut out)?;
            } else if full {
                linter::help_agents::generate_full(&cmd, &mut out)?;
            } else {
                linter::help_agents::generate_index(&cmd, &mut out)?;
            }
        }

        Commands::HelpJson { command } => {
            use clap::CommandFactory;
            let cmd = Cli::command();
            let mut out = std::io::stdout();
            linter::help_agents::generate_json(&cmd, command.as_deref(), &mut out)?;
        }

        Commands::Tree { path } => {
            if !path.is_dir() {
                anyhow::bail!("Not a directory: {}", path.display());
            }
            println!("{}", path.display());
            print_tree(&path, "")?;
        }

        Commands::Version => {
            print!("{}", render_version_info());
        }
    }

    Ok(())
}

/// Render multi-line build provenance for `flint version`.
///
/// Pulled out as a pure function so we can unit-test the layout without
/// shelling out. All inputs come from `env!()` at compile time — fields
/// are constants from the caller's perspective and the function is here
/// purely to centralize formatting.
fn render_version_info() -> String {
    format!(
        "flint {version}\n  Build:       {build}\n  Target:      {target}\n  Fleet sync:  {sync_commit} ({sync_date})\n",
        version = env!("CARGO_PKG_VERSION"),
        build = env!("BUILD_TIMESTAMP"),
        target = env!("TARGET_TRIPLE"),
        sync_commit = env!("FLEET_SYNC_COMMIT"),
        sync_date = env!("FLEET_SYNC_DATE"),
    )
}

/// Generate the pre-commit hook script tailored to the install flags.
///
/// - `strict = false` (default): non-blocking. flint runs with `--hook-mode`
///   and the script always exits 0 — diagnostics print but commits never block.
/// - `strict = true`: errors block the commit. flint runs without
///   `--hook-mode` so its native exit code propagates; on failure the script
///   prints a hint about `--no-verify` and exits 1.
/// - `json = true`: flint emits JSON diagnostics (`--format json`). Useful for
///   piping into other tools or CI logs.
fn build_pre_commit_script(strict: bool, json: bool) -> String {
    // Marker string used by `hooks uninstall` to detect a flint-authored hook.
    let header = "# flint pre-commit hook (generated by `flint hooks install`)";

    let format_flag = if json { " --format=json" } else { "" };
    // Non-strict mode uses --hook-mode so flint always exits 0.
    let hook_mode_flag = if strict { "" } else { " --hook-mode" };

    let banner = if strict {
        "flint: running pre-commit validation (strict — errors will block commits)"
    } else {
        "flint: running pre-commit validation (warnings only, never blocks)"
    };

    let on_fail = if strict {
        // Errors block. Print a hint then propagate.
        r#"echo ""  >&2
        echo "flint: errors found — commit blocked." >&2
        echo "  • Bypass once: git commit --no-verify" >&2
        echo "  • Switch to non-blocking: flint hooks install --force" >&2
        exit 1"#
    } else {
        ":  # informational only — never block"
    };

    format!(
        r#"#!/bin/sh
{header}
#
# Runs `flint check{format_flag}{hook_mode_flag}` against staged YAML files
# (or the whole repo if none are staged) when you `git commit`.
# Strict mode: {strict}. JSON output: {json}.
#
# Remove with: flint hooks uninstall

set -e

if ! command -v flint >/dev/null 2>&1; then
    echo "flint: pre-commit hook installed but 'flint' not found on PATH — skipping" >&2
    exit 0
fi

staged_yaml="$(git diff --cached --name-only --diff-filter=ACMRT \
    -- '*.yml' '*.yaml' 2>/dev/null || true)"

echo "{banner}"

# `flint check` takes a single path, so use `xargs -n 1` to invoke it
# once per staged YAML file. Track failure across the batch via a marker
# file (subshell exit codes can't propagate from `while`/`xargs` reliably).
fail_marker="$(mktemp)"
trap 'rm -f "$fail_marker"' EXIT

if [ -n "$staged_yaml" ]; then
    echo "$staged_yaml" | while IFS= read -r f; do
        [ -n "$f" ] || continue
        [ -f "$f" ] || continue
        flint check{format_flag}{hook_mode_flag} "$f" || echo 1 > "$fail_marker"
    done
else
    flint check{format_flag}{hook_mode_flag} . || echo 1 > "$fail_marker"
fi

if [ -s "$fail_marker" ]; then
    {on_fail}
fi

exit 0
"#,
        header = header,
        format_flag = format_flag,
        hook_mode_flag = hook_mode_flag,
        banner = banner,
        strict = strict,
        json = json,
        on_fail = on_fail,
    )
}

/// Locate the `.git/hooks/` directory for the current repo.
///
/// Walks up from the current working directory looking for a `.git` entry.
/// Errors out with a clear message if not in a git repo.
fn find_git_hooks_dir() -> Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        let git = dir.join(".git");
        if git.is_dir() {
            return Ok(git.join("hooks"));
        }
        // Some setups (worktrees, submodules) make `.git` a file pointing
        // at the real gitdir.
        if git.is_file() {
            let contents = std::fs::read_to_string(&git)?;
            if let Some(gitdir) = contents.strip_prefix("gitdir: ") {
                let gitdir = PathBuf::from(gitdir.trim());
                let resolved = if gitdir.is_absolute() {
                    gitdir
                } else {
                    dir.join(gitdir)
                };
                return Ok(resolved.join("hooks"));
            }
        }
        if !dir.pop() {
            anyhow::bail!(
                "Not inside a git repository (no .git found from current directory upward)"
            );
        }
    }
}

fn install_pre_commit_hook(force: bool, strict: bool, json: bool) -> Result<()> {
    use colored::Colorize;
    use std::os::unix::fs::PermissionsExt;

    let hooks_dir = find_git_hooks_dir()?;
    std::fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join("pre-commit");

    if hook_path.exists() && !force {
        let existing = std::fs::read_to_string(&hook_path).unwrap_or_default();
        if existing.contains("flint pre-commit hook") {
            println!(
                "{} flint pre-commit hook already installed at {}\n  Re-run with --force to overwrite (e.g. to switch modes).",
                "ℹ".blue(),
                hook_path.display()
            );
            return Ok(());
        }
        anyhow::bail!(
            "A pre-commit hook already exists at {} and was not authored by flint.\nRe-run with --force to overwrite, or move it aside first.",
            hook_path.display()
        );
    }

    let script = build_pre_commit_script(strict, json);
    std::fs::write(&hook_path, script)?;
    let mut perms = std::fs::metadata(&hook_path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&hook_path, perms)?;

    let mode_label = match (strict, json) {
        (false, false) => "non-blocking, text",
        (false, true) => "non-blocking, JSON",
        (true, false) => "strict (blocks on errors), text",
        (true, true) => "strict (blocks on errors), JSON",
    };

    println!(
        "{} Installed flint pre-commit hook at {} ({})",
        "✓".green(),
        hook_path.display(),
        mode_label
    );
    if strict {
        println!("  • Errors block the commit; warnings/info pass through.");
        println!("  • Bypass with `git commit --no-verify`.");
    } else {
        println!("  • Diagnostics print on every commit, but the hook never blocks.");
    }
    println!("  • Remove with: flint hooks uninstall");
    Ok(())
}

fn uninstall_pre_commit_hook() -> Result<()> {
    use colored::Colorize;

    let hook_path = find_git_hooks_dir()?.join("pre-commit");
    if !hook_path.exists() {
        println!("{} No pre-commit hook found at {}", "ℹ".blue(), hook_path.display());
        return Ok(());
    }
    let contents = std::fs::read_to_string(&hook_path).unwrap_or_default();
    if !contents.contains("flint pre-commit hook") {
        anyhow::bail!(
            "Pre-commit hook at {} was not authored by flint — refusing to remove it.\nDelete it manually if intended.",
            hook_path.display()
        );
    }
    std::fs::remove_file(&hook_path)?;
    println!("{} Removed flint pre-commit hook from {}", "✓".green(), hook_path.display());
    Ok(())
}

/// Count YAML files in a directory recursively.
fn walkdir_count(dir: &std::path::Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += walkdir_count(&path);
            } else if let Some(ext) = path.extension() {
                if ext == "yml" || ext == "yaml" {
                    count += 1;
                }
            }
        }
    }
    count
}

/// Print a directory tree, excluding .git, .DS_Store, node_modules, target, dist.
fn print_tree(dir: &std::path::Path, prefix: &str) -> anyhow::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let s = name.to_string_lossy();
            !matches!(
                s.as_ref(),
                ".git" | ".DS_Store" | "node_modules" | "target" | "dist" | ".gitkeep"
            )
        })
        .collect();

    entries.sort_by_key(|e| {
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        (!is_dir, e.file_name()) // dirs first, then alphabetical
    });

    let total = entries.len();
    for (i, entry) in entries.iter().enumerate() {
        let is_last = i == total - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last { "    " } else { "│   " };
        let name = entry.file_name();

        println!("{}{}{}", prefix, connector, name.to_string_lossy());

        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            print_tree(&entry.path(), &format!("{}{}", prefix, child_prefix))?;
        }
    }

    Ok(())
}

/// Reconcile the user's `--format` choice against `--git`.
///
/// `--git` only makes sense for markdown output, so:
/// - default (`text`) silently upgrades to `markdown` (simplest CI invocation).
/// - explicit `markdown` stays as-is.
/// - any other explicit format (e.g. `json`) is a user error and bails.
fn resolve_format_for_git(git: bool, format: &str) -> anyhow::Result<String> {
    if !git {
        return Ok(format.to_string());
    }
    match format {
        "markdown" | "text" => Ok("markdown".to_string()),
        other => anyhow::bail!(
            "--git implies --format markdown; got --format {other}. \
             Drop --format to use the default, or set it to markdown explicitly."
        ),
    }
}

/// Hidden marker emitted at the top of every markdown body. Lets a
/// future dedup pass identify flint-authored PR comments (e.g. for
/// edit-in-place updates) without trying to fingerprint the body.
const MARKDOWN_MARKER: &str = "<!-- flint-check-report -->";

/// PR context detected from CI environment variables.
#[derive(Debug, PartialEq, Eq)]
enum PrContext {
    /// GitHub Actions running on a `pull_request` (or `pull_request_target`)
    /// event. PR number parsed from `GITHUB_REF` (`refs/pull/<n>/merge`).
    GithubActions { pr_number: String },
}

/// Extract the PR number from a GitHub Actions `GITHUB_REF` value.
///
/// On `pull_request` events the ref looks like `refs/pull/123/merge`.
/// Returns `None` for push/tag refs and other formats so the caller
/// can fall through to a clear "not a PR build" error.
fn parse_github_pr_ref(github_ref: &str) -> Option<&str> {
    let rest = github_ref.strip_prefix("refs/pull/")?;
    let num = rest.split('/').next()?;
    if num.is_empty() || !num.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(num)
}

/// Detect the current PR context from CI env vars.
///
/// Reads `GITHUB_ACTIONS` and `GITHUB_REF` from the process environment.
/// Returns an error explaining what's missing so CI logs surface the
/// concrete reason (wrong event, missing env var, not in CI).
fn detect_pr_context() -> anyhow::Result<PrContext> {
    if std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true") {
        let github_ref = std::env::var("GITHUB_REF").map_err(|_| {
            anyhow::anyhow!("GITHUB_ACTIONS is set but GITHUB_REF is not — cannot find PR number")
        })?;
        let pr = parse_github_pr_ref(&github_ref).ok_or_else(|| {
            anyhow::anyhow!(
                "GITHUB_REF={github_ref} is not a pull_request ref (expected refs/pull/<n>/merge)"
            )
        })?;
        return Ok(PrContext::GithubActions {
            pr_number: pr.to_string(),
        });
    }
    if std::env::var("GITLAB_CI").as_deref() == Ok("true") {
        anyhow::bail!("GitLab CI is not yet supported in --git mode (GitHub Actions only for now)");
    }
    anyhow::bail!("no supported CI environment detected (expected GITHUB_ACTIONS=true)");
}

/// Post the markdown body as a PR comment via `gh pr comment`.
///
/// Shells out to `gh` to inherit the user's existing auth (the standard
/// `GITHUB_TOKEN` on GitHub-hosted runners). Stdout from `gh` propagates
/// to the user; non-zero exit becomes an error so the caller can log it.
/// Note: every invocation creates a new comment — dedup/edit-in-place is
/// a follow-up (see `MARKDOWN_MARKER`).
fn post_pr_comment(body: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let ctx = detect_pr_context()?;
    let PrContext::GithubActions { pr_number } = ctx;

    let mut child = Command::new("gh")
        .args(["pr", "comment", &pr_number, "--body-file", "-"])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn `gh` (is it installed and on PATH?): {e}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(body.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("`gh pr comment` exited with status {status}");
    }
    eprintln!("flint: posted check report to PR #{pr_number}");
    Ok(())
}

/// Map a lint run's counts to a process exit code.
///
/// Default (no `--detailed-exitcodes`): `0` unless errors were found, in which
/// case `1`. Warnings and infos do not affect the exit code — preserves the
/// pre-flag behavior so existing CI doesn't break.
///
/// With `--detailed-exitcodes` (terraform/fleet-plan convention):
/// `0` = no findings of any severity, `2` = any finding. Engine errors
/// propagate via `anyhow::Result` from `main` and surface as `1` regardless
/// of this flag.
fn resolve_exit_code(detailed_exitcodes: bool, errors: usize, total_findings: usize) -> i32 {
    if detailed_exitcodes {
        if total_findings > 0 {
            2
        } else {
            0
        }
    } else if errors > 0 {
        1
    } else {
        0
    }
}

/// Render a lint run as a GitHub-flavored-markdown comment body.
///
/// Output is a `## flint check` heading, a summary line, and one
/// `<details>` block per file that has at least one finding. The
/// per-file body is a table of `severity | line | rule | message` rows,
/// so a CI step can pipe this straight into `gh pr comment --body-file -`.
fn render_markdown_report(
    files: &[(String, &linter::error::LintReport)],
    files_linted: usize,
    errors: usize,
    warnings: usize,
    infos: usize,
    heading: Option<&str>,
) -> String {
    let mut out = String::new();
    // HTML comment is invisible in rendered markdown. Future dedup logic
    // (edit-in-place rather than stacking comments) can grep PR comments
    // for this marker to identify flint-authored bodies.
    out.push_str(MARKDOWN_MARKER);
    out.push('\n');
    out.push_str("## ");
    out.push_str(heading.unwrap_or("flint check"));
    out.push_str("\n\n");

    let total = errors + warnings + infos;
    if total == 0 {
        out.push_str(&format!(
            "✓ No issues found across {} file(s).\n",
            files_linted
        ));
        return out;
    }

    out.push_str(&format!(
        "**Summary:** {} error(s), {} warning(s), {} info across {} file(s).\n\n",
        errors, warnings, infos, files_linted
    ));

    for (path, report) in files {
        if report.total_issues() == 0 {
            continue;
        }
        out.push_str(&format!(
            "<details><summary><code>{}</code> — {} error(s), {} warning(s), {} info</summary>\n\n",
            md_escape(path),
            report.errors.len(),
            report.warnings.len(),
            report.infos.len()
        ));
        out.push_str("| Severity | Line | Rule | Message |\n");
        out.push_str("| --- | --- | --- | --- |\n");

        let rows = report
            .errors
            .iter()
            .chain(report.warnings.iter())
            .chain(report.infos.iter());
        for e in rows {
            let icon = match e.severity {
                linter::Severity::Error => "❌ error",
                linter::Severity::Warning => "⚠️ warning",
                linter::Severity::Info => "ℹ️ info",
            };
            let loc = match (e.line, e.column) {
                (Some(l), Some(c)) => format!("{}:{}", l, c),
                (Some(l), None) => l.to_string(),
                _ => "—".to_string(),
            };
            let rule = e
                .rule_code
                .as_deref()
                .map(|c| format!("`{}`", md_escape(c)))
                .unwrap_or_else(|| "—".to_string());
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                icon,
                loc,
                rule,
                md_escape(&e.message)
            ));
        }
        out.push_str("\n</details>\n\n");
    }
    out
}

/// Escape characters that would break a GitHub-markdown table cell.
///
/// Pipes terminate cells, backticks toggle inline code, and backslashes
/// need doubling so they don't escape the *next* character we emit.
fn md_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('`', "\\`")
        .replace('\n', " ")
}

/// Convert a LintReport to a JSON value for structured output.
fn lint_report_to_json(file_path: &str, report: &linter::error::LintReport) -> serde_json::Value {
    let to_json = |errors: &[linter::LintError]| -> Vec<serde_json::Value> {
        errors
            .iter()
            .map(|e| {
                let mut obj = serde_json::json!({
                    "message": e.message,
                    "severity": match e.severity {
                        linter::Severity::Error => "error",
                        linter::Severity::Warning => "warning",
                        linter::Severity::Info => "info",
                    },
                });
                let m = obj.as_object_mut().unwrap();
                if let Some(line) = e.line {
                    m.insert("line".into(), serde_json::json!(line));
                }
                if let Some(col) = e.column {
                    m.insert("column".into(), serde_json::json!(col));
                }
                if let Some(ref code) = e.rule_code {
                    m.insert("rule".into(), serde_json::json!(code));
                }
                if let Some(ref help) = e.help {
                    m.insert("help".into(), serde_json::json!(help));
                }
                if let Some(ref suggestion) = e.suggestion {
                    m.insert("suggestion".into(), serde_json::json!(suggestion));
                }
                if let Some(ref ctx) = e.context {
                    m.insert("context".into(), serde_json::json!(ctx));
                }
                obj
            })
            .collect()
    };

    let mut diagnostics = to_json(&report.errors);
    diagnostics.extend(to_json(&report.warnings));
    diagnostics.extend(to_json(&report.infos));

    serde_json::json!({
        "path": file_path,
        "diagnostics": diagnostics,
        "counts": {
            "errors": report.errors.len(),
            "warnings": report.warnings.len(),
            "infos": report.infos.len(),
        }
    })
}

/// Apply auto-fixable suggestions to a file.
///
/// Collects all fixable errors (Safe, or Unsafe if `include_unsafe` is true),
/// applies them bottom-up to preserve line numbers, and writes the file back.
/// Returns the number of fixes applied.
fn apply_fixes(
    file_path: &std::path::Path,
    report: &linter::error::LintReport,
    include_unsafe: bool,
) -> anyhow::Result<usize> {
    use linter::error::FixSafety;

    // Collect fixable errors from all severity levels
    let all_errors: Vec<&linter::LintError> = report
        .errors
        .iter()
        .chain(report.warnings.iter())
        .chain(report.infos.iter())
        .collect();

    // Filter to fixable errors with line/context info
    let mut fixes: Vec<(&linter::LintError, &str)> = all_errors
        .iter()
        .filter_map(|e| {
            let suggestion = e.suggestion.as_deref()?;
            let safety = e.fix_safety.as_ref()?;

            match safety {
                FixSafety::Safe => Some((*e, suggestion)),
                FixSafety::Unsafe if include_unsafe => Some((*e, suggestion)),
                _ => None,
            }
        })
        .filter(|(e, _)| e.line.is_some() && e.context.is_some())
        .collect();

    if fixes.is_empty() {
        return Ok(0);
    }

    let source = std::fs::read_to_string(file_path)?;
    let mut lines: Vec<String> = source.lines().map(|l| l.to_string()).collect();

    // Sort by line number descending so replacements don't shift earlier lines
    fixes.sort_by(|a, b| b.0.line.cmp(&a.0.line));

    let mut applied = 0;

    for (error, suggestion) in &fixes {
        let line_idx = error.line.unwrap() - 1; // 0-indexed
        if line_idx >= lines.len() {
            continue;
        }

        let context = error.context.as_deref().unwrap();
        let line = &lines[line_idx];

        // Replace the context value with the suggestion on this line
        if let Some(pos) = line.find(context) {
            let mut new_line = String::new();
            new_line.push_str(&line[..pos]);
            new_line.push_str(suggestion);
            new_line.push_str(&line[pos + context.len()..]);
            lines[line_idx] = new_line;
            applied += 1;
        }
    }

    if applied > 0 {
        let mut output = lines.join("\n");
        // Preserve trailing newline if original had one
        if source.ends_with('\n') {
            output.push('\n');
        }
        std::fs::write(file_path, output)?;
    }

    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: the original generator used `xargs flint check` which
    /// batches every staged YAML into a single invocation. `flint check`
    /// only takes one positional path, so multi-file commits failed with
    /// "unexpected argument" and the hook short-circuited. Generated scripts
    /// must invoke flint once per file.
    #[test]
    fn pre_commit_script_invokes_flint_per_file() {
        for (strict, json) in [(false, false), (true, false), (false, true), (true, true)] {
            let script = build_pre_commit_script(strict, json);
            assert!(
                !script.contains("| xargs flint check"),
                "regression: hook script (strict={strict}, json={json}) batches files via xargs"
            );
            assert!(
                script.contains("while IFS= read -r f; do"),
                "hook script (strict={strict}, json={json}) should invoke flint once per file"
            );
            // The per-file invocation must quote the file path.
            assert!(
                script.contains("flint check") && script.contains("\"$f\""),
                "hook script (strict={strict}, json={json}) should pass each staged file as a quoted positional arg"
            );
        }
    }

    #[test]
    fn pre_commit_script_modes_render_distinct_banners() {
        let non_strict = build_pre_commit_script(false, false);
        let strict = build_pre_commit_script(true, false);
        assert!(non_strict.contains("warnings only, never blocks"));
        assert!(strict.contains("strict — errors will block commits"));
        assert_ne!(non_strict, strict, "modes must produce different scripts");
    }

    #[test]
    fn pre_commit_script_strict_mode_has_explicit_block_path() {
        // In strict mode the script must propagate flint's failure through
        // the marker file and emit the human-readable "errors found —
        // commit blocked" message before exiting 1.
        let strict = build_pre_commit_script(true, false);
        assert!(strict.contains("errors found — commit blocked."));
        assert!(strict.contains("exit 1"));
    }

    #[test]
    fn pre_commit_script_non_strict_never_blocks() {
        // Non-strict scripts must NEVER include `exit 1` — even if flint
        // returns an error, the marker check is a no-op.
        let non_strict = build_pre_commit_script(false, false);
        assert!(
            !non_strict.contains("exit 1"),
            "non-strict hook must not contain `exit 1` — it always passes"
        );
    }

    #[test]
    fn pre_commit_script_json_mode_passes_format_flag() {
        let json = build_pre_commit_script(false, true);
        assert!(
            json.contains("--format=json"),
            "json mode must pass --format=json to flint check"
        );
        let text = build_pre_commit_script(false, false);
        assert!(
            !text.contains("--format=json"),
            "non-json mode must not pass --format=json"
        );
    }

    // Exit code matrix — preserves the legacy 0/1 contract by default and
    // promotes warnings/infos to exit-2 only when --detailed-exitcodes is set.
    #[test]
    fn exit_code_default_zero_when_clean() {
        assert_eq!(resolve_exit_code(false, 0, 0), 0);
    }

    #[test]
    fn exit_code_default_one_on_errors_only() {
        // Legacy contract: warnings/infos must NOT trigger non-zero exit.
        assert_eq!(resolve_exit_code(false, 0, 5), 0);
        assert_eq!(resolve_exit_code(false, 3, 8), 1);
    }

    #[test]
    fn exit_code_detailed_zero_when_no_findings() {
        assert_eq!(resolve_exit_code(true, 0, 0), 0);
    }

    #[test]
    fn exit_code_detailed_two_on_any_finding() {
        // Warnings alone, infos alone, and errors alone all surface as 2.
        assert_eq!(resolve_exit_code(true, 0, 1), 2);
        assert_eq!(resolve_exit_code(true, 1, 1), 2);
        assert_eq!(resolve_exit_code(true, 0, 7), 2);
    }

    fn make_report() -> linter::error::LintReport {
        use linter::error::{FixSafety, LintError, LintReport, Severity};
        let mut r = LintReport::new();
        let mut err = LintError::error("missing required field `platforms`", "teams/ws.yml");
        err.severity = Severity::Error;
        err.line = Some(12);
        err.column = Some(5);
        err.rule_code = Some("required-fields".into());
        r.add(err);

        let mut warn = LintError::warning("interval `1s` is very short", "teams/ws.yml");
        warn.line = Some(47);
        warn.rule_code = Some("interval-validation".into());
        warn.fix_safety = Some(FixSafety::Display);
        r.add(warn);
        r
    }

    #[test]
    fn markdown_zero_findings_emits_clean_summary() {
        let report = linter::error::LintReport::new();
        let out = render_markdown_report(&[("teams/ws.yml".into(), &report)], 1, 0, 0, 0, None);
        assert!(out.starts_with(MARKDOWN_MARKER), "marker must lead the body");
        assert!(out.contains("## flint check"));
        assert!(out.contains("✓ No issues found"));
        // No <details> blocks when clean.
        assert!(!out.contains("<details>"));
    }

    #[test]
    fn markdown_marker_is_present_on_every_render() {
        // Marker must appear regardless of findings — dedup/edit-in-place
        // logic relies on it for both clean and dirty runs.
        let empty = linter::error::LintReport::new();
        let busy = make_report();
        let clean = render_markdown_report(&[("a.yml".into(), &empty)], 1, 0, 0, 0, None);
        let dirty = render_markdown_report(&[("b.yml".into(), &busy)], 1, 1, 1, 0, None);
        assert!(clean.contains(MARKDOWN_MARKER));
        assert!(dirty.contains(MARKDOWN_MARKER));
    }

    #[test]
    fn markdown_custom_heading_replaces_default() {
        // Monorepo case: two flint reports in one PR comment need
        // distinguishable headings so reviewers can tell them apart.
        let empty = linter::error::LintReport::new();
        let out = render_markdown_report(
            &[("a.yml".into(), &empty)],
            1,
            0,
            0,
            0,
            Some("Staging diff"),
        );
        assert!(out.contains("## Staging diff"));
        assert!(
            !out.contains("## flint check"),
            "custom heading must REPLACE the default, not append"
        );
        // Marker still leads regardless of heading.
        assert!(out.starts_with(MARKDOWN_MARKER));
    }

    // Parser for $GITHUB_REF on pull_request events. Critical that bogus
    // values (push refs, tags, malformed strings) return None so the
    // caller surfaces a clear error rather than posting to PR #0 or
    // truncating non-numeric noise.
    #[test]
    fn parse_github_pr_ref_accepts_pull_request_refs() {
        assert_eq!(parse_github_pr_ref("refs/pull/1/merge"), Some("1"));
        assert_eq!(parse_github_pr_ref("refs/pull/12345/merge"), Some("12345"));
        // pull_request_target also uses /merge but `/head` shows up too.
        assert_eq!(parse_github_pr_ref("refs/pull/42/head"), Some("42"));
    }

    #[test]
    fn parse_github_pr_ref_rejects_non_pr_refs() {
        // Push to main.
        assert_eq!(parse_github_pr_ref("refs/heads/main"), None);
        // Tag push.
        assert_eq!(parse_github_pr_ref("refs/tags/v1.0.0"), None);
        // Empty.
        assert_eq!(parse_github_pr_ref(""), None);
        // Right prefix, but non-numeric PR id (would silently post to a
        // nonsense PR if we didn't validate).
        assert_eq!(parse_github_pr_ref("refs/pull/abc/merge"), None);
        // Right prefix, empty number segment.
        assert_eq!(parse_github_pr_ref("refs/pull//merge"), None);
    }

    // `--git` validation. The pure resolver is testable; the dispatcher
    // just delegates to it.
    #[test]
    fn git_flag_off_passes_format_through_unchanged() {
        assert_eq!(resolve_format_for_git(false, "text").unwrap(), "text");
        assert_eq!(resolve_format_for_git(false, "json").unwrap(), "json");
        assert_eq!(
            resolve_format_for_git(false, "markdown").unwrap(),
            "markdown"
        );
    }

    #[test]
    fn git_flag_upgrades_default_text_to_markdown() {
        // The 90%-case CI invocation is `flint check --git` — no explicit
        // format. We silently pick markdown rather than erroring on the
        // default value the user didn't set.
        assert_eq!(resolve_format_for_git(true, "text").unwrap(), "markdown");
    }

    #[test]
    fn git_flag_keeps_explicit_markdown() {
        assert_eq!(
            resolve_format_for_git(true, "markdown").unwrap(),
            "markdown"
        );
    }

    #[test]
    fn version_info_renders_all_required_fields() {
        // Triage messages from users include this output verbatim, so the
        // four labeled fields (Build / Target / Fleet sync) must always
        // be present. The exact values come from build.rs env injection.
        let out = render_version_info();
        assert!(out.starts_with("flint "), "leading line must be 'flint <version>'");
        assert!(out.contains("Build:"));
        assert!(out.contains("Target:"));
        assert!(out.contains("Fleet sync:"));
        // Trailing newline so shells can append cleanly.
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn version_info_includes_pkg_version() {
        let out = render_version_info();
        assert!(
            out.contains(env!("CARGO_PKG_VERSION")),
            "version line must include CARGO_PKG_VERSION"
        );
    }

    #[test]
    fn git_flag_rejects_incompatible_format() {
        let err = resolve_format_for_git(true, "json").unwrap_err().to_string();
        assert!(
            err.contains("--git implies --format markdown"),
            "error must explain the conflict, got: {err}"
        );
        assert!(err.contains("json"), "error must echo the offending format");
    }

    #[test]
    fn markdown_renders_per_file_details_with_table() {
        let report = make_report();
        let pairs = vec![("teams/ws.yml".to_string(), &report)];
        let out = render_markdown_report(&pairs, 1, 1, 1, 0, None);

        assert!(out.contains("**Summary:** 1 error(s), 1 warning(s)"));
        assert!(out.contains("<details><summary><code>teams/ws.yml</code>"));
        assert!(out.contains("| Severity | Line | Rule | Message |"));
        assert!(out.contains("❌ error"));
        assert!(out.contains("⚠️ warning"));
        assert!(out.contains("`required-fields`"));
        assert!(out.contains("12:5"));
        // Files with zero issues must be skipped, but our single file has issues.
        assert_eq!(out.matches("<details>").count(), 1);
    }

    #[test]
    fn markdown_skips_files_with_no_findings() {
        let empty = linter::error::LintReport::new();
        let busy = make_report();
        let pairs = vec![
            ("a.yml".to_string(), &empty),
            ("b.yml".to_string(), &busy),
        ];
        let out = render_markdown_report(&pairs, 2, 1, 1, 0, None);
        assert!(!out.contains("a.yml"), "empty files must not appear");
        assert!(out.contains("b.yml"));
    }

    #[test]
    fn markdown_escapes_pipes_and_backticks_in_messages() {
        // Diagnostic messages can contain `|` (e.g. enum lists) and backticks
        // (quoting field names). Both would break a markdown table row if
        // emitted verbatim.
        use linter::error::{LintError, LintReport};
        let mut r = LintReport::new();
        let mut err = LintError::error(
            "value must be one of `a` | `b` | `c`",
            "teams/ws.yml",
        );
        err.line = Some(1);
        err.rule_code = Some("enum".into());
        r.add(err);

        let out = render_markdown_report(&[("teams/ws.yml".into(), &r)], 1, 1, 0, 0, None);
        assert!(out.contains("\\|"), "pipes must be escaped in cells");
        assert!(out.contains("\\`"), "backticks must be escaped in cells");
    }
}
