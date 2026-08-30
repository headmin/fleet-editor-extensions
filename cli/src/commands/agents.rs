//! `flint help-agents` / `flint help-json` / `flint setup-agent` — the
//! machine-readable CLI reference for AI agents.

use crate::args::{Cli, HelpAgentsArgs, HelpJsonArgs};

pub(crate) fn run_help_agents(args: HelpAgentsArgs) -> anyhow::Result<()> {
    use clap::CommandFactory;
    let cmd = Cli::command();
    let mut out = std::io::stdout();
    if let Some(tool) = args.sop {
        crate::help_agents::generate_sop(&tool, &mut out)?;
    } else if let Some(path) = args.command {
        crate::help_agents::generate_command(&cmd, &path, &mut out)?;
    } else if args.full {
        crate::help_agents::generate_full(&cmd, &mut out)?;
    } else {
        crate::help_agents::generate_index(&cmd, &mut out)?;
    }
    Ok(())
}

pub(crate) fn run_help_json(args: HelpJsonArgs) -> anyhow::Result<()> {
    use clap::CommandFactory;
    let cmd = Cli::command();
    let mut out = std::io::stdout();
    crate::help_agents::generate_json(&cmd, args.command.as_deref(), &mut out)?;
    Ok(())
}

pub(crate) fn run_setup_agent() -> anyhow::Result<()> {
    crate::help_agents::install_skill(env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
