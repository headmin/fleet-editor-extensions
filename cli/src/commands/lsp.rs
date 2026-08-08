//! `flint lsp` — start the language server (spawned by editor extensions).

use crate::args::LspArgs;
use flint_lsp as lsp;

pub(crate) async fn run(args: LspArgs) -> anyhow::Result<()> {
    // Set up logging if debug mode is enabled
    if args.debug {
        eprintln!("Fleet LSP server starting in debug mode...");
        // TODO: Set up tracing/logging to stderr
    }

    // Start the LSP server - this blocks until the client disconnects
    // Note: stdio transport is always used, the --stdio flag is accepted for compatibility
    let _ = args.stdio;
    lsp::start_server().await?;
    Ok(())
}
