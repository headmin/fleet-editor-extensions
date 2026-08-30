//! Flint CLI — Fleet GitOps YAML linter and language server.
//!
//! Thin command dispatcher that delegates to `flint-lint` (linting engine)
//! and `flint-lsp` (language server). See `flint help-ai` for agent discovery.
//!
//! Layout: argument definitions live in [`args`], one module per subcommand
//! under [`commands`], report rendering under [`output`], CI posting under
//! [`ci`], and the interactive wiring flows under [`interactive`].

// The CLI is the terminal frontend — printing is its whole job. The lint is
// enabled workspace-wide so the LIBRARY crates stay renderable-but-silent
// (`LintReport::render` returns a String); this is the one crate that opts
// back in.
#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "this crate is the terminal frontend; the library crates stay silent"
)]

mod args;
mod ci;
mod commands;
mod help_agents;
mod interactive;
mod output;
mod overlay;

use anyhow::Result;
use args::{Cli, Commands};
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Check(a) => commands::check::run(a)?,
        Commands::DryRun(a) => commands::dry_run::run(a)?,
        Commands::Hooks { action } => commands::hooks::run(action)?,
        Commands::History(a) => commands::history::run(a)?,
        Commands::Lsp(a) => commands::lsp::run(a).await?,
        Commands::Init(a) => commands::init::run(a)?,
        Commands::ListRules(a) => commands::list_rules::run(a)?,
        Commands::Migrate(a) => commands::migrate::run(a)?,
        Commands::SetupAgent => commands::agents::run_setup_agent()?,
        Commands::HelpAgents(a) => commands::agents::run_help_agents(a)?,
        Commands::HelpJson(a) => commands::agents::run_help_json(a)?,
        Commands::Gen { what } => commands::gen::run(what)?,
        Commands::Query(a) => {
            let new = if a.policy {
                "flint gen policy --from <file.sql>"
            } else {
                "flint gen query --from <file.sql>"
            };
            commands::deprecation_warning("flint query", new);
            commands::gen::query::run(a)?
        }
        Commands::New(a) => {
            commands::deprecation_warning("flint new", &format!("flint gen {}", a.kind));
            commands::gen::new::run(a)?
        }
        Commands::Profile(a) => {
            commands::deprecation_warning("flint profile", "flint gen profile --from <file>");
            commands::gen::profile::run(a)?
        }
        Commands::App(a) => {
            commands::deprecation_warning("flint app", "flint gen software --from <installer>");
            commands::gen::app::run(a)?
        }
        Commands::Pkg(a) => {
            let new = if a.policy {
                "flint gen policy --from <file.pkg>"
            } else if a.scripts.is_some() {
                "flint gen scripts --from <file.pkg> -o <dir>"
            } else {
                "flint gen software --from <file.pkg>"
            };
            commands::deprecation_warning("flint pkg", new);
            commands::gen::pkg::run(a)?
        }
        Commands::Fma { what } => commands::fma::run(what)?,
        Commands::Fleet { what } => commands::fleet::run(what)?,
        Commands::Tree(a) => commands::tree::run(a)?,
        Commands::Paths(a) => commands::paths::run(a)?,
        Commands::Version => commands::version::run(),
    }

    Ok(())
}
