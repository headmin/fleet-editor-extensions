//! LSP (Language Server Protocol) server for Fleet GitOps YAML validation.
//!
//! This module provides real-time semantic validation for Fleet GitOps YAML files
//! through the standard Language Server Protocol.

pub mod backend;
pub mod code_actions;
pub mod completion;
pub mod diagnostics;
pub mod hover;
pub mod position;
pub mod schema;
pub mod semantic_tokens;
pub mod symbols;
pub mod workspace;

use anyhow::Result;
use tower_lsp::{LspService, Server};

use backend::FleetLspBackend;
use crate::linter::Linter;

/// Start the LSP server using stdio transport.
///
/// This function blocks until the client disconnects.
pub async fn start_server() -> Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| FleetLspBackend::new(client, Linter::new()));

    Server::new(stdin, stdout, socket).serve(service).await;
    Ok(())
}
