//! CLI argument definitions — the `clap` surface of every flint command.
//!
//! Each subcommand's flags live in a dedicated `#[derive(Args)]` struct so the
//! command modules (`crate::commands::*`) take one typed argument instead of a
//! long parameter list. Doc comments and `#[arg]` attributes are verbatim from
//! the original single-file CLI, so `--help` and `help-json` output are
//! byte-identical.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "flint")]
#[command(version = concat!(
    env!("CARGO_PKG_VERSION"), "+", env!("BUILD_TIMESTAMP"),
    " (Fleet sync: ", env!("FLEET_SYNC_COMMIT"), ", ", env!("FLEET_SYNC_DATE"), ")"
))]
#[command(about = "Flint — Fleet GitOps YAML linter and language server", long_about = None)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Check (lint) YAML file(s) with Fleet-specific validation
    #[command(alias = "lint")]
    Check(CheckArgs),

    /// Local, server-free dry-run: lint the whole repo and report whether it
    /// would likely pass `fleetctl gitops`. The local equivalent of
    /// `fleetctl gitops --dry-run` — no Fleet server, no fleetctl. Errors are
    /// blocking (exit 2); warnings are advisory unless --strict.
    #[command(visible_alias = "dryrun")]
    DryRun(DryRunArgs),

    /// Manage git hooks for non-blocking flint validation in a Fleet GitOps repo.
    Hooks {
        #[command(subcommand)]
        action: HooksAction,
    },

    /// Replay today's rules against past commits, and mine remediation
    /// commits for conventions no rule encodes yet.
    ///
    /// Default mode is a replay: for each first-parent commit it reconstructs
    /// the tree and runs the engine, then reports "red windows" — the runs of
    /// commits in which a rule would have fired, and the commit that closed
    /// each one. A code with two or more CLOSED windows is a repeat failure,
    /// which is prioritisation evidence rather than a judgement call.
    ///
    /// `--suggest-patterns` switches to archaeology: it mines commits that
    /// look like remediation and proposes `[[patterns]]` guardrails for
    /// conventions that recurred. That output is heuristic, is emitted
    /// commented out, and is never written to your config.
    History(HistoryArgs),

    /// Start language server (called by editor extensions, not directly)
    #[command(hide = true)]
    Lsp(LspArgs),

    /// Initialize Fleet linter configuration
    ///
    /// Creates a `fleetlint.toml` in the current directory (the hidden
    /// `.fleetlint.toml` spelling is still read). Auto-detects your Fleet
    /// GitOps structure, then walks your directories asking which ones to
    /// lint — showing how many files each choice puts in scope, and warning
    /// when you exclude a directory that live config still references.
    /// Answers become directory globs, never extension globs.
    ///
    /// Use --no-interactive to skip the questions and write an unnarrowed
    /// config.
    Init(InitArgs),

    /// List all available lint rules
    ListRules(ListRulesArgs),

    /// Generate a migration report for upgrading Fleet GitOps YAML
    Migrate(MigrateArgs),

    /// Output CLI reference for AI agents (default: command index)
    #[command(name = "help-agents", alias = "help-ai")]
    HelpAgents(HelpAgentsArgs),

    /// Install AI agent skill files (.claude/skills/)
    #[command(name = "setup-agent")]
    SetupAgent,

    /// Output CLI schema as JSON for tooling integration
    #[command(name = "help-json", hide = true)]
    HelpJson(HelpJsonArgs),

    /// Generate copy-pasteable Fleet GitOps YAML — from real artifacts
    /// (installers, profiles, .sql queries) or as blank templates.
    ///
    /// The `--from` source's extension picks the generator (e.g.
    /// `gen policy --from app.pkg` emits an installed-check policy;
    /// `--from query.sql` a policy around that query); omitting `--from`
    /// emits a blank template where one exists.
    Gen {
        #[command(subcommand)]
        what: GenKind,
    },

    /// Deprecated: use `flint gen query --from <file.sql>`
    /// (or `gen policy --from <file.sql>` for --policy).
    #[command(hide = true)]
    Query(QueryArgs),

    /// Deprecated: use `flint gen <kind>` (e.g. `flint gen policy`).
    #[command(hide = true)]
    New(NewArgs),

    /// Deprecated: use `flint gen profile --from <file>`.
    #[command(hide = true)]
    Profile(ProfileArgs),

    /// Deprecated: use `flint gen software --from <installer>`.
    #[command(hide = true)]
    App(AppArgs),

    /// Deprecated: use `flint gen software --from <file.pkg>` (or
    /// `gen policy` / `gen scripts` for the --policy/--scripts modes).
    #[command(hide = true)]
    Pkg(PkgArgs),

    /// Fleet Maintained Apps: search slugs, show app details, list recent
    /// updates, and refresh the local registry from fmalibrary.com.
    Fma {
        #[command(subcommand)]
        what: FmaKind,
    },

    /// Read-only views of a Fleet instance (never writes). Connection from
    /// `.fleetlint.toml [fleet]`, `~/.config/flint/config.toml`, `FLEET_URL`/
    /// `FLEET_API_TOKEN` (also via ./.env), with `op://` secret references.
    Fleet {
        #[command(subcommand)]
        what: FleetKind,
    },

    /// Show the directory tree of a Fleet GitOps repo
    Tree(TreeArgs),

    /// Report broken `path:` references and where the files moved to.
    ///
    /// Scans a Fleet GitOps repo (or a single file) for `path:` references
    /// whose target file is missing, and prints each as a copy-paste
    /// before→after block. References with a single unambiguous match are
    /// auto-fixable; pass --fix to rewrite them in place.
    Paths(PathsArgs),

    /// Print detailed build information (version, build date, target triple,
    /// Fleet sync info). For a single-line version string, use `flint --version`.
    Version,
}

#[derive(Args)]
pub(crate) struct CheckArgs {
    /// File(s) or directory to lint. Accepts multiple paths — e.g. the
    /// staged files a pre-commit hook passes (`pass_filenames: true`).
    /// Defaults to the current directory.
    #[arg(default_value = ".", num_args = 1..)]
    pub(crate) paths: Vec<PathBuf>,

    /// Automatically apply safe fixes
    #[arg(long)]
    pub(crate) fix: bool,

    /// Also apply fixes that may change semantics (requires --fix)
    #[arg(long)]
    pub(crate) unsafe_fixes: bool,

    /// Output format
    #[arg(short, long, default_value = "text", value_parser = ["text", "json", "markdown", "github"])]
    pub(crate) format: String,

    /// Run as a non-blocking git hook: print diagnostics but always exit 0,
    /// so warnings/errors don't block commits. Suitable for `.git/hooks/pre-commit`.
    #[arg(long)]
    pub(crate) hook_mode: bool,

    /// Pre-commit mode: analysis stays repo-wide (the cross-file graph pass
    /// needs the whole repo), but only findings touching a git-staged file —
    /// directly or via a cross-file `related` path — are reported and affect
    /// the exit code. Pre-existing findings elsewhere are summarized, not
    /// blocking. The staged set is read with `--no-renames` so a rename's
    /// OLD path (the one references still point at) stays in scope (ADR-010).
    #[arg(long)]
    pub(crate) staged: bool,

    /// Use terraform-style exit codes: 0 = no findings, 1 = engine error,
    /// 2 = findings detected (any severity). Without this flag, only
    /// errors trigger a non-zero exit. Ignored when --hook-mode is set.
    #[arg(long)]
    pub(crate) detailed_exitcodes: bool,

    /// CI mode: auto-post the markdown report as a PR comment via `gh`.
    /// Currently supports GitHub Actions (detected via $GITHUB_ACTIONS
    /// and $GITHUB_REF). Implies --format markdown; errors if --format
    /// is set to something else. Requires `gh` on PATH and a token with
    /// PR-comment scope. On post failure, the body still prints to
    /// stdout so the CI logs preserve it.
    #[arg(long)]
    pub(crate) git: bool,

    /// Override the markdown heading. Useful when a monorepo PR posts
    /// multiple flint reports (e.g. one per sub-project) and readers
    /// need to tell them apart. Only affects `--format markdown`.
    #[arg(long, value_name = "TEXT")]
    pub(crate) heading: Option<String>,

    /// Base YAML manifest to deep-merge before linting. Used together
    /// with --env to lint the resolved overlay result instead of the
    /// base and overlay in isolation. Catches deprecated keys and
    /// structural errors that only appear after merge.
    #[arg(long, value_name = "PATH", requires = "env")]
    pub(crate) base: Option<PathBuf>,

    /// Environment overlay YAML, deep-merged onto --base. Overlay
    /// values win for everything except nested mappings, which recurse.
    /// Lists are replaced, not concatenated (matches `yq` semantics).
    #[arg(long, value_name = "PATH", requires = "base")]
    pub(crate) env: Option<PathBuf>,

    /// Glob pattern of files/dirs to skip (repeatable). Merged with
    /// `files.exclude` in .fleetlint.toml. E.g.
    /// `--exclude '**/tools-scripts/fleet-templates/**'`.
    #[arg(long, value_name = "GLOB")]
    pub(crate) exclude: Vec<String>,
}

#[derive(Args)]
pub(crate) struct HistoryArgs {
    /// Repo directory (default: current directory)
    #[arg(default_value = ".")]
    pub(crate) path: PathBuf,

    /// Replay from this ref (exclusive) up to HEAD, instead of the last
    /// `--max` commits.
    #[arg(long, value_name = "REF")]
    pub(crate) since: Option<String>,

    /// Maximum first-parent commits to examine.
    #[arg(long, default_value_t = 200, value_name = "N")]
    pub(crate) max: usize,

    /// Mine remediation commits for candidate `[[patterns]]` guardrails
    /// instead of replaying the rules.
    #[arg(long)]
    pub(crate) suggest_patterns: bool,

    /// With --suggest-patterns: how many times a convention must have been
    /// repaired by hand before it is proposed. One repair is an anecdote.
    #[arg(long, default_value_t = 2, value_name = "N", requires = "suggest_patterns")]
    pub(crate) min_occurrences: usize,

    /// Path to the `gitops-oracle` binary. When given, each replayed tree is
    /// also put through Fleet's own parser and the two verdicts are diffed,
    /// so the report measures correctness rather than self-consistency.
    ///
    /// Dev/CI only — the oracle is not shipped with flint.
    #[arg(long, value_name = "PATH", conflicts_with = "suggest_patterns")]
    pub(crate) oracle: Option<PathBuf>,

    /// Replay each tree under ITS OWN committed `.fleetlint.toml` instead of
    /// today's.
    ///
    /// The default applies the current config to every tree, because "today's
    /// rules against yesterday's trees" should hold scope fixed too: a
    /// directory the repo has since declared out of scope would otherwise
    /// pollute the whole history. Use this to ask the different question,
    /// "what would flint have said at the time".
    #[arg(long)]
    pub(crate) scope_as_committed: bool,

    /// Gate the run against a stored scorecard, for CI.
    ///
    /// Compares this replay to the baseline at PATH and exits 2 if rule
    /// quality regressed: a rule that newly claims blocking where Fleet
    /// accepts, or a Fleet complaint flint has newly gone silent on. Writes
    /// the file and passes if it does not exist yet.
    ///
    /// Only new KEYS gate. Counts move with the range replayed, so they are
    /// reported and never failed on.
    #[arg(long, value_name = "PATH", conflicts_with = "suggest_patterns")]
    pub(crate) gate: Option<PathBuf>,

    /// Overwrite the --gate baseline with this run instead of comparing.
    #[arg(long, requires = "gate")]
    pub(crate) update_baseline: bool,

    /// Emit JSON — the form an agent consumes.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct DryRunArgs {
    /// Repo directory (default: current directory)
    #[arg(default_value = ".")]
    pub(crate) path: PathBuf,

    /// Treat warnings as blocking too (zero-tolerance gate).
    #[arg(long)]
    pub(crate) strict: bool,

    /// Glob of files/dirs to skip (repeatable), merged with
    /// `files.exclude` in .fleetlint.toml.
    #[arg(long, value_name = "GLOB")]
    pub(crate) exclude: Vec<String>,

    /// Emit the verdict as JSON.
    #[arg(long)]
    pub(crate) json: bool,

    /// Refresh `.fleet-snapshot.json` from the Fleet server before linting.
    ///
    /// A snapshot can prove something EXISTS on the server; it cannot prove
    /// something is absent, only that it was absent when captured. So a
    /// package uploaded after the last capture is reported as missing and
    /// blocks the run. This re-reads server state first, so "not uploaded"
    /// means now rather than whenever the file was last written.
    ///
    /// Opt-in on purpose: dry-run is otherwise offline and deterministic.
    /// Needs Fleet credentials; if the refresh fails the existing snapshot is
    /// used and the reason is printed, so a network blip degrades the answer
    /// instead of failing the run.
    #[arg(long)]
    pub(crate) refresh_snapshot: bool,

    /// Treat every software package as already uploaded to the server.
    ///
    /// Drops the one finding that exists only because a `.fleet-snapshot.json`
    /// was consulted — "hash is not uploaded". Useful while the installer is
    /// mid-upload, or when the snapshot is older than the work in progress and
    /// refreshing is not possible.
    ///
    /// It suppresses evidence rather than supplying it: if the package really
    /// is absent, `fleetctl gitops` still fails with "package not found with
    /// hash". Prefer `--refresh-snapshot`, which answers the question instead
    /// of skipping it.
    #[arg(long, conflicts_with = "refresh_snapshot")]
    pub(crate) assume_uploaded: bool,
}

#[derive(Args)]
pub(crate) struct LspArgs {
    /// Enable debug logging to stderr
    #[arg(long)]
    pub(crate) debug: bool,

    /// Use stdio transport (default, accepted for compatibility)
    #[arg(long)]
    pub(crate) stdio: bool,
}

#[derive(Args)]
pub(crate) struct InitArgs {
    /// Output path for config file (default: fleetlint.toml)
    #[arg(short, long)]
    pub(crate) output: Option<PathBuf>,

    /// Skip interactive prompts, use detected/default values
    #[arg(long)]
    pub(crate) no_interactive: bool,

    /// Force overwrite existing config
    #[arg(short, long)]
    pub(crate) force: bool,
}

#[derive(Args)]
pub(crate) struct ListRulesArgs {
    /// Output format (text, json)
    #[arg(short, long, default_value = "text")]
    pub(crate) format: String,
}

#[derive(Args)]
pub(crate) struct MigrateArgs {
    /// Root directory of the GitOps repo
    pub(crate) path: PathBuf,

    /// Target Fleet version (e.g., "4.85.0" or "latest")
    #[arg(long, default_value = "latest")]
    pub(crate) target_version: String,
}

#[derive(Args)]
pub(crate) struct HelpAgentsArgs {
    /// Show full detail for a specific command (dot notation)
    #[arg(long)]
    pub(crate) command: Option<String>,

    /// Show standard operating procedures for a tool (lint, migrate, lsp,
    /// hooks, author, paths, software, add-field)
    #[arg(long)]
    pub(crate) sop: Option<String>,

    /// Output the complete reference (all commands, all flags)
    #[arg(long)]
    pub(crate) full: bool,

    /// Deprecated: use `flint setup-agent`.
    #[arg(long, hide = true)]
    pub(crate) install_skill: bool,
}

#[derive(Args)]
pub(crate) struct HelpJsonArgs {
    /// Command path to scope output (dot notation, e.g. check)
    pub(crate) command: Option<String>,
}

#[derive(Args)]
pub(crate) struct QueryArgs {
    /// Path to the .sql file
    pub(crate) path: PathBuf,

    /// Emit a policy stanza (name/query/platform) instead of a query
    #[arg(long)]
    pub(crate) policy: bool,

    /// Append the generated stanza to this file (created if absent)
    #[arg(short, long, value_name = "FILE")]
    pub(crate) output: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct NewArgs {
    /// What to create
    #[arg(value_parser = ["profile", "fleet", "policy", "query", "label"])]
    pub(crate) kind: String,

    /// Write to this file instead of stdout
    #[arg(short, long, value_name = "FILE")]
    pub(crate) output: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct ProfileArgs {
    /// Path to the .mobileconfig or .json file
    pub(crate) path: PathBuf,

    /// Append the generated entry to this file (created if absent)
    #[arg(short, long, value_name = "FILE")]
    pub(crate) output: Option<PathBuf>,

    /// Add commented labels_include_any / labels_exclude_any stubs
    #[arg(long)]
    pub(crate) full: bool,

    /// Mitigation: rewrite this .mobileconfig's top-level PayloadUUID with a
    /// fresh UUID (uses `uuidgen`). Resolves duplicate-PayloadUUID findings.
    #[arg(long)]
    pub(crate) regen_uuid: bool,

    /// After generating, interactively insert the entry into a chosen
    /// fleet/team file (path computed relative to that file).
    #[arg(long)]
    pub(crate) wire: bool,
}

#[derive(Args)]
pub(crate) struct AppArgs {
    /// Path to the installer
    pub(crate) path: PathBuf,

    /// Append the generated block to this file (created if absent)
    #[arg(short, long, value_name = "FILE")]
    pub(crate) output: Option<PathBuf>,

    /// Emit a full software stanza (url placeholder + commented stubs)
    #[arg(long)]
    pub(crate) full: bool,

    /// After generating, interactively insert the stanza into a chosen
    /// fleet/team file under software.packages.
    #[arg(long)]
    pub(crate) wire: bool,
}

#[derive(Args)]
pub(crate) struct PkgArgs {
    /// Path to the .pkg file
    pub(crate) path: PathBuf,

    /// Append the generated block to this file (created if absent)
    #[arg(short, long, value_name = "FILE")]
    pub(crate) output: Option<PathBuf>,

    /// Emit a full software stanza (url placeholder, self_service, and
    /// commented-out scripts/labels/categories) instead of the minimal
    /// comment + hash_sha256 block.
    #[arg(long)]
    pub(crate) full: bool,

    /// After generating, interactively insert the stanza into a chosen
    /// fleet/team file under software.packages.
    #[arg(long)]
    pub(crate) wire: bool,

    /// Refresh an existing stanza in this file: re-read the .pkg and update
    /// its hash_sha256, version, and url filename (matched by identifier).
    #[arg(long, value_name = "FILE", conflicts_with_all = ["output", "wire"])]
    pub(crate) update: Option<PathBuf>,

    /// Write default install.sh + uninstall.sh for this .pkg into this dir
    /// (the uninstall is pinned to the package identifier).
    #[arg(long, value_name = "DIR", conflicts_with_all = ["output", "wire", "update"])]
    pub(crate) scripts: Option<PathBuf>,

    /// Emit an "installed & up to date" Fleet policy (via package_receipts)
    /// instead of the software block. Honors -o to append.
    #[arg(long, conflicts_with_all = ["wire", "update", "scripts", "full"])]
    pub(crate) policy: bool,

    /// With --policy: target the `apps` table by bundle_identifier instead
    /// of `package_receipts` by package_id (for packages that install a
    /// .app). Verify the identifier matches the app's bundle id.
    #[arg(long, requires = "policy")]
    pub(crate) apps: bool,

    /// With --policy: add an `install_software` automation referencing this
    /// package by its sha256, so Fleet auto-installs it on hosts that FAIL
    /// the policy. Turns the audit policy into an enforcement policy. The
    /// package must also be listed under software in the same team.
    #[arg(long, requires = "policy")]
    pub(crate) enforce: bool,

    /// With --policy: emit a query that fails ONLY when an outdated copy is
    /// installed (passes if the package is absent) — patch existing installs
    /// without forcing a fresh install. Default policies also fail when the
    /// package is missing.
    #[arg(long = "outdated-only", requires = "policy")]
    pub(crate) outdated_only: bool,

    /// With --policy: verify install by checking this file exists on disk
    /// (the macOS `file` table) instead of package_receipts/apps — a proxy
    /// for packages that don't register a clean receipt (fonts, configs).
    /// Existence-only. E.g. --file '/Applications/YourApp.app/Contents/Info.plist'.
    #[arg(long, value_name = "PATH", requires = "policy", conflicts_with_all = ["apps", "outdated_only"])]
    pub(crate) file: Option<String>,

    /// Add `setup_experience: true` to the software stanza — install the
    /// package during Setup Assistant (OOBE). Applies to the generated
    /// block, -o, and --wire (skips the interactive prompt).
    #[arg(long = "setup-experience")]
    pub(crate) setup_experience: bool,

    /// Write a complete standalone software file (the top-level mapping
    /// referenced via `path:`) named
    /// `<name>.package.yml` (derived from the installer, version stripped);
    /// collisions get a `-2`/`-3` suffix so repeated runs never overwrite. Goes next
    /// to the .pkg, into the -o directory, or at the exact -o path when it
    /// ends in .yml. Honors --full and --setup-experience.
    #[arg(long = "yml", conflicts_with_all = ["wire", "update", "scripts", "policy"])]
    pub(crate) yml: bool,
}

#[derive(Args)]
pub(crate) struct TreeArgs {
    /// Root directory (default: current directory)
    #[arg(default_value = ".")]
    pub(crate) path: PathBuf,
}

#[derive(Args)]
pub(crate) struct PathsArgs {
    /// File or directory to scan (default: current directory)
    #[arg(default_value = ".")]
    pub(crate) path: PathBuf,

    /// Rewrite unambiguous references in place
    #[arg(long)]
    pub(crate) fix: bool,

    /// Report artifacts (profiles, scripts, software…) that exist on disk
    /// but no fleet config references — and suggest how to wire each.
    #[arg(long, conflicts_with = "fix")]
    pub(crate) unwired: bool,

    /// With --unwired: walk each unwired artifact group and prompt to wire
    /// it into each fleet/team file (yes/no/skip), inserting the construct
    /// with a comment. Modifies files only on an explicit "yes".
    #[arg(short, long, requires = "unwired")]
    pub(crate) interactive: bool,

    /// In interactive per-file wiring, when a labels_* answer is left blank,
    /// emit a placeholder instead of omitting the key. `--label-stubs`
    /// (or `=blank`) writes the empty key (`labels_include_any:`), ready to
    /// fill; `--label-stubs=comment` writes a commented stub
    /// (`# labels_include_any:` …) — inert until uncommented.
    #[arg(
        long,
        value_name = "MODE",
        num_args = 0..=1,
        default_missing_value = "blank",
        value_parser = ["blank", "comment"],
        requires = "interactive"
    )]
    pub(crate) label_stubs: Option<String>,

    /// Limit interactive wiring to fleet/team files matching this glob
    /// (e.g. "fleets/team-*.yml" or "team-*"). Matched against each file's
    /// path and name.
    #[arg(long, value_name = "GLOB", requires = "unwired")]
    pub(crate) only: Option<String>,

    /// With --unwired: one tab-separated record per artifact
    /// (`path  section  wire-value`) instead of the YAML blocks, so the
    /// report can be filtered — `flint paths --unwired --oneline | grep pilot`.
    #[arg(long, requires = "unwired", conflicts_with = "interactive")]
    pub(crate) oneline: bool,

    /// With --unwired: emit a ready-to-run instruction per artifact for an AI
    /// agent — the file to edit, the key to insert under, the exact line, and
    /// the two commands that prove it worked. Pair with `--only` to scope to
    /// one fleet file, and filter with grep to target a single artifact.
    #[arg(long, requires = "unwired", conflicts_with_all = ["interactive", "oneline"])]
    pub(crate) prompt: bool,
}

#[derive(Subcommand)]
pub(crate) enum HooksAction {
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

        /// Install a blocking pre-push hook that runs the local dry-run
        /// (`flint dry-run .`) over the whole repo before every push, instead
        /// of the staged-file pre-commit hook. Catches gitops dry-run failures
        /// before they reach the pipeline.
        #[arg(long = "pre-push")]
        pre_push: bool,
    },
    /// Remove flint's pre-commit hook from the current git repo.
    Uninstall,
}

/// The `flint gen` artifact kinds — one verb for flint's generator face.
#[derive(Subcommand)]
pub(crate) enum GenKind {
    /// Software metadata block from an installer
    /// (.pkg/.deb/.ipa/.msi/.rpm/.exe/.tar.gz). Identifier/version are
    /// extracted where the format and available tools allow; the sha256 is
    /// always computed. (.dmg is not a Fleet custom-package format —
    /// repackage to .pkg.)
    Software(GenSoftwareArgs),

    /// Policy stanza: from a .pkg an "installed & up to date" check (via
    /// package_receipts); from a .sql a policy around that query (platform
    /// inferred from the osquery tables); with no --from, a blank template.
    Policy(GenPolicyArgs),

    /// `configuration_profiles` entry from a .mobileconfig or DDM .json —
    /// or a blank profile template with no --from.
    ///
    /// flint checks the Fleet side of a profile (is it referenced, wired, and
    /// its PayloadUUID unique). It does not inspect the payload. If `contour`
    /// is installed it is also validated against Apple's schema and any
    /// findings are printed to stderr, leaving stdout copy-pasteable.
    /// Entirely optional — with contour absent the output is unchanged.
    /// See https://github.com/macadmins/contour
    Profile(GenProfileArgs),

    /// Query stanza from a .sql file (platform inferred from the osquery
    /// tables it references) — or a blank template with no --from.
    Query(GenQueryArgs),

    /// Default install.sh + uninstall.sh for a .pkg (the uninstall is pinned
    /// to the package identifier). Writes into the -o directory.
    Scripts(GenScriptsArgs),

    /// Blank fleet (team) file template.
    Fleet(GenTemplateArgs),

    /// Blank label template.
    Label(GenTemplateArgs),
}

#[derive(Args)]
pub(crate) struct GenSoftwareArgs {
    /// Source installer (.pkg/.deb/.ipa/.msi/.rpm/.exe/.tar.gz)
    #[arg(long, value_name = "INSTALLER")]
    pub(crate) from: PathBuf,

    /// Append the generated block to this file (created if absent)
    #[arg(short, long, value_name = "FILE")]
    pub(crate) output: Option<PathBuf>,

    /// Emit a full software stanza (url placeholder, self_service, and
    /// commented-out scripts/labels/categories) instead of the minimal
    /// comment + hash_sha256 block.
    #[arg(long)]
    pub(crate) full: bool,

    /// After generating, interactively insert the stanza into a chosen
    /// fleet/team file under software.packages.
    #[arg(long)]
    pub(crate) wire: bool,

    /// Add `setup_experience: true` to the software stanza — install the
    /// package during Setup Assistant (OOBE).
    #[arg(long = "setup-experience")]
    pub(crate) setup_experience: bool,

    /// Write a complete standalone software file (the top-level mapping
    /// referenced via `path:`) named
    /// `<name>.package.yml` (derived from the installer, version stripped);
    /// collisions get a `-2`/`-3` suffix so repeated runs never overwrite. Goes next
    /// to the installer, into the -o directory, or at the exact -o path when
    /// it ends in .yml. Honors --full and --setup-experience. (.pkg only.)
    #[arg(long, conflicts_with_all = ["wire", "update"])]
    pub(crate) standalone: bool,

    /// Refresh an existing stanza in this file: re-read the installer and
    /// update its hash_sha256, version, and url filename (matched by
    /// identifier). (.pkg only.)
    #[arg(long, value_name = "FILE", conflicts_with_all = ["output", "wire"])]
    pub(crate) update: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct GenPolicyArgs {
    /// Source artifact: a .pkg (installed-check policy) or .sql (policy
    /// around that query). Omit for a blank policy template.
    #[arg(long, value_name = "PATH")]
    pub(crate) from: Option<PathBuf>,

    /// Append the generated stanza to this file (created if absent)
    #[arg(short, long, value_name = "FILE")]
    pub(crate) output: Option<PathBuf>,

    /// With a .pkg source: target the `apps` table by bundle_identifier
    /// instead of `package_receipts` by package_id (for packages that
    /// install a .app). Verify the identifier matches the app's bundle id.
    #[arg(long, requires = "from")]
    pub(crate) apps: bool,

    /// With a .pkg source: add an `install_software` automation referencing
    /// this package by its sha256, so Fleet auto-installs it on hosts that
    /// FAIL the policy. The package must also be listed under software in
    /// the same team.
    #[arg(long, requires = "from")]
    pub(crate) enforce: bool,

    /// With a .pkg source: emit a query that fails ONLY when an outdated
    /// copy is installed (passes if the package is absent).
    #[arg(long = "outdated-only", requires = "from")]
    pub(crate) outdated_only: bool,

    /// With a .pkg source: verify install by checking this file exists on
    /// disk (the macOS `file` table) instead of package_receipts/apps.
    /// Existence-only.
    #[arg(long, value_name = "PATH", requires = "from", conflicts_with_all = ["apps", "outdated_only"])]
    pub(crate) file: Option<String>,
}

#[derive(Args)]
pub(crate) struct GenProfileArgs {
    /// Source .mobileconfig or DDM .json. Omit for a blank profile template.
    #[arg(long, value_name = "PATH")]
    pub(crate) from: Option<PathBuf>,

    /// Append the generated entry to this file (created if absent)
    #[arg(short, long, value_name = "FILE")]
    pub(crate) output: Option<PathBuf>,

    /// Add commented labels_include_any / labels_exclude_any stubs
    #[arg(long, requires = "from")]
    pub(crate) full: bool,

    /// Mitigation: rewrite this .mobileconfig's top-level PayloadUUID with a
    /// fresh UUID (uses `uuidgen`). Resolves duplicate-PayloadUUID findings.
    #[arg(long, requires = "from")]
    pub(crate) regen_uuid: bool,

    /// After generating, interactively insert the entry into a chosen
    /// fleet/team file (path computed relative to that file).
    #[arg(long, requires = "from")]
    pub(crate) wire: bool,
}

#[derive(Args)]
pub(crate) struct GenQueryArgs {
    /// Source .sql file. Omit for a blank query template.
    #[arg(long, value_name = "PATH")]
    pub(crate) from: Option<PathBuf>,

    /// Append the generated stanza to this file (created if absent)
    #[arg(short, long, value_name = "FILE")]
    pub(crate) output: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct GenScriptsArgs {
    /// Source .pkg the scripts are generated for
    #[arg(long, value_name = "PKG")]
    pub(crate) from: PathBuf,

    /// Directory to write install.sh + uninstall.sh into
    #[arg(short, long, value_name = "DIR")]
    pub(crate) output: PathBuf,
}

#[derive(Args)]
pub(crate) struct GenTemplateArgs {
    /// Write to this file instead of stdout
    #[arg(short, long, value_name = "FILE")]
    pub(crate) output: Option<PathBuf>,
}

/// The `flint fma` subcommands.
#[derive(Subcommand)]
pub(crate) enum FmaKind {
    /// Search apps by name substring; prints ready-to-paste slugs.
    Search {
        /// Search term (matched against app names)
        term: String,

        /// Emit results as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show one app's details (platforms, latest known version, installer URL).
    Show {
        /// App name or slug (e.g. "slack" or "slack/darwin")
        slug: String,

        /// Emit as JSON
        #[arg(long)]
        json: bool,
    },

    /// List recent version updates from the fmalibrary.com feed cache.
    Latest {
        /// Only updates from the last N days
        #[arg(long, default_value = "7")]
        days: u32,

        /// Emit as JSON
        #[arg(long)]
        json: bool,
    },

    /// Fetch the fmalibrary.com feed and refresh the local registry cache
    /// (~/.cache/flint/fma-cache.toml) used by lint, the LSP, and search.
    Refresh,
}

/// The `flint fleet` read-only subcommands.
#[derive(Subcommand)]
pub(crate) enum FleetKind {
    /// Check the connection: server version and license tier.
    Status,

    /// Walk the connection setup one step at a time, timing each.
    ///
    /// `status` answers "does it work?"; this answers "where does it stop?".
    /// It repeats the steps the language server performs while starting —
    /// find the config, parse it, read the feature flags, resolve `url` and
    /// `token` (which may shell out to `op`), then reach the server — and
    /// prints each result as it completes. If a step blocks, the last line
    /// printed names it, so a hung editor does not need a process sampler to
    /// diagnose.
    Doctor,

    /// Software titles on the instance (name, version, install status).
    Software {
        /// Scope to a team id
        #[arg(long, value_name = "ID")]
        team: Option<u32>,

        /// Only titles available for install
        #[arg(long)]
        available: bool,

        /// Emit the raw API response as JSON
        #[arg(long)]
        json: bool,
    },

    /// Fleet Maintained Apps catalog as YOUR instance offers it
    /// (cross-check slugs against what this Fleet version supports).
    Fma {
        /// Emit the raw API response as JSON
        #[arg(long)]
        json: bool,
    },

    /// Labels defined on the instance.
    Labels {
        /// Emit the raw API response as JSON
        #[arg(long)]
        json: bool,
    },

    /// Teams (fleets) defined on the instance.
    Teams {
        /// Emit the raw API response as JSON
        #[arg(long)]
        json: bool,
    },

    /// Write a `.fleet-snapshot.json` recording server state the repo cannot
    /// supply, so rules that can only warn today are able to gate.
    ///
    /// Commit the file. It holds names and presence flags only — never a
    /// token, and the server as a bare hostname.
    Snapshot {
        /// Where to write (default: ./.fleet-snapshot.json)
        #[arg(long, value_name = "PATH")]
        out: Option<std::path::PathBuf>,

        /// Print the snapshot instead of writing it
        #[arg(long)]
        stdout: bool,
    },
}
