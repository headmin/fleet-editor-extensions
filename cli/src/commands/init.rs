//! `flint init` — scaffold a fleetlint.toml.

use crate::args::InitArgs;
use crate::interactive::scope::TerminalPrompts;
use flint_lint as linter;

pub(crate) fn run(args: InitArgs) -> anyhow::Result<()> {
    let current_dir = std::env::current_dir()?;
    let prompts = TerminalPrompts;
    let prompts: Option<&dyn linter::InitPrompts> = if args.no_interactive {
        None
    } else {
        Some(&prompts)
    };
    linter::init_config(&current_dir, args.output, prompts, args.force)?;
    Ok(())
}
