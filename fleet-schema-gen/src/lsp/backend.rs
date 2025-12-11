//! LSP backend implementation for Fleet GitOps validation.

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeActionParams, CodeActionProviderCapability, CodeActionResponse,
    Diagnostic, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, InitializeParams, InitializeResult, InitializedParams,
    MessageType, ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
    Url, Position, Range, DiagnosticSeverity,
};
use tower_lsp::{Client, LanguageServer};

use crate::linter::Linter;
use super::code_actions::generate_code_actions;
use super::diagnostics::lint_error_to_diagnostic;

/// Fleet LSP backend that handles document events and publishes diagnostics.
pub struct FleetLspBackend {
    /// LSP client for sending notifications.
    client: Client,
    /// Document content cache, keyed by URI.
    documents: DashMap<String, String>,
    /// The Fleet GitOps linter.
    linter: Linter,
}

impl FleetLspBackend {
    /// Create a new Fleet LSP backend.
    pub fn new(client: Client, linter: Linter) -> Self {
        Self {
            client,
            documents: DashMap::new(),
            linter,
        }
    }

    /// Handle document change - lint and publish diagnostics.
    async fn on_change(&self, uri: String, content: String) {
        // Cache the document content
        self.documents.insert(uri.clone(), content.clone());

        // Lint the document
        let diagnostics = self.lint_document(&uri, &content);

        // Parse URI for publishing
        if let Ok(url) = Url::parse(&uri) {
            self.client
                .publish_diagnostics(url, diagnostics, None)
                .await;
        }
    }

    /// Lint a document and return LSP diagnostics.
    fn lint_document(&self, uri: &str, content: &str) -> Vec<Diagnostic> {
        // Extract file path from URI for the linter
        let file_path = Url::parse(uri)
            .ok()
            .and_then(|u| u.to_file_path().ok())
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| uri.to_string());

        // Use the linter's lint_content method
        match self.linter.lint_content(content, std::path::Path::new(&file_path)) {
            Ok(report) => {
                // Convert all errors to diagnostics
                let mut diagnostics = Vec::new();

                for error in &report.errors {
                    diagnostics.push(lint_error_to_diagnostic(error, content));
                }
                for warning in &report.warnings {
                    diagnostics.push(lint_error_to_diagnostic(warning, content));
                }
                for info in &report.infos {
                    diagnostics.push(lint_error_to_diagnostic(info, content));
                }

                diagnostics
            }
            Err(e) => {
                // Parse error - create a single diagnostic at the start
                vec![Diagnostic {
                    range: Range {
                        start: Position { line: 0, character: 0 },
                        end: Position { line: 0, character: 0 },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("fleet-lsp".to_string()),
                    message: format!("Failed to parse YAML: {}", e),
                    ..Default::default()
                }]
            }
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for FleetLspBackend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                // Enable code actions for quick-fixes
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "fleet-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Fleet LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let content = params.text_document.text;
        self.on_change(uri, content).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        // We request FULL sync, so there's always exactly one change with full content
        if let Some(change) = params.content_changes.into_iter().next() {
            self.on_change(uri, change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();

        // Remove from cache
        self.documents.remove(&uri);

        // Clear diagnostics
        if let Ok(url) = Url::parse(&uri) {
            self.client.publish_diagnostics(url, vec![], None).await;
        }
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let actions = generate_code_actions(&params);
        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }
}
