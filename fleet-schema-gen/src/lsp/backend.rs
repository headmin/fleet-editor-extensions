//! LSP backend implementation for Fleet GitOps validation.

use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeActionParams, CodeActionProviderCapability, CodeActionResponse,
    CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbol, DocumentSymbolParams,
    DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse,
    Hover, HoverParams, HoverProviderCapability,
    InitializeParams, InitializeResult, InitializedParams,
    MessageType, OneOf, ServerCapabilities, ServerInfo, TextDocumentSyncCapability,
    TextDocumentSyncKind, Url, Position, Range, DiagnosticSeverity,
    SemanticTokens, SemanticTokensFullOptions, SemanticTokensOptions,
    SemanticTokensParams, SemanticTokensResult, SemanticTokensServerCapabilities,
};
use tower_lsp::{Client, LanguageServer};

use crate::linter::{FleetLintConfig, Linter};
use super::code_actions::generate_code_actions;
use super::completion::complete_at_with_context;
use super::diagnostics::lint_error_to_diagnostic;
use super::hover::hover_at;
use super::semantic_tokens::{compute_semantic_tokens, create_legend};
use super::symbols::document_symbols;
use super::workspace::{get_path_definition, validate_path_references};

/// Fleet LSP backend that handles document events and publishes diagnostics.
pub struct FleetLspBackend {
    /// LSP client for sending notifications.
    client: Client,
    /// Document content cache, keyed by URI.
    documents: DashMap<String, String>,
    /// The Fleet GitOps linter.
    linter: RwLock<Linter>,
    /// Workspace root path.
    workspace_root: RwLock<Option<PathBuf>>,
}

impl FleetLspBackend {
    /// Create a new Fleet LSP backend.
    pub fn new(client: Client, linter: Linter) -> Self {
        Self {
            client,
            documents: DashMap::new(),
            linter: RwLock::new(linter),
            workspace_root: RwLock::new(None),
        }
    }

    /// Load configuration from workspace root.
    fn load_config(&self, workspace_root: &PathBuf) {
        if let Some((config_path, config)) = FleetLintConfig::find_and_load(workspace_root) {
            // Update linter with new config
            if let Ok(mut linter) = self.linter.write() {
                linter.set_config(config);
            }

            // Log that we found a config
            let client = self.client.clone();
            let path = config_path.display().to_string();
            tokio::spawn(async move {
                client.log_message(
                    MessageType::INFO,
                    format!("Loaded Fleet config from {}", path)
                ).await;
            });
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

        let file_path_buf = std::path::PathBuf::from(&file_path);

        // Use the linter's lint_content method
        let linter = self.linter.read().unwrap();
        let mut diagnostics = match linter.lint_content(content, std::path::Path::new(&file_path)) {
            Ok(report) => {
                // Convert all errors to diagnostics
                let mut diags = Vec::new();

                for error in &report.errors {
                    diags.push(lint_error_to_diagnostic(error, content));
                }
                for warning in &report.warnings {
                    diags.push(lint_error_to_diagnostic(warning, content));
                }
                for info in &report.infos {
                    diags.push(lint_error_to_diagnostic(info, content));
                }

                diags
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
        };

        // Add path reference validation diagnostics
        let workspace_root = file_path_buf.parent();
        diagnostics.extend(validate_path_references(
            content,
            &file_path_buf,
            workspace_root,
        ));

        diagnostics
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for FleetLspBackend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Store workspace root and load config
        if let Some(root_uri) = params.root_uri {
            if let Ok(path) = root_uri.to_file_path() {
                if let Ok(mut workspace_root) = self.workspace_root.write() {
                    *workspace_root = Some(path.clone());
                }
                self.load_config(&path);
            }
        } else if let Some(folders) = params.workspace_folders {
            // Use first workspace folder
            if let Some(folder) = folders.first() {
                if let Ok(path) = folder.uri.to_file_path() {
                    if let Ok(mut workspace_root) = self.workspace_root.write() {
                        *workspace_root = Some(path.clone());
                    }
                    self.load_config(&path);
                }
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                // Enable hover for documentation tooltips
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                // Enable code actions for quick-fixes
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                // Enable autocompletion
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ":".to_string(),
                        " ".to_string(),
                        "-".to_string(),
                    ]),
                    ..Default::default()
                }),
                // Enable document symbols for outline view
                document_symbol_provider: Some(OneOf::Left(true)),
                // Enable go-to-definition for path references
                definition_provider: Some(OneOf::Left(true)),
                // Enable semantic tokens for syntax highlighting
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: create_legend(),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            range: None,
                            ..Default::default()
                        },
                    ),
                ),
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

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri.to_string();
        let position = params.text_document_position_params.position;

        // Get document content from cache
        if let Some(content) = self.documents.get(&uri) {
            Ok(hover_at(&content, position))
        } else {
            Ok(None)
        }
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let position = params.text_document_position.position;

        // Get document content from cache
        if let Some(content) = self.documents.get(&uri) {
            // Get file path for file path completions
            let file_path = Url::parse(&uri)
                .ok()
                .and_then(|u| u.to_file_path().ok());

            // Get workspace root
            let workspace_root = self.workspace_root.read().ok().and_then(|r| r.clone());

            let items = complete_at_with_context(
                &content,
                position,
                file_path.as_deref(),
                workspace_root.as_deref(),
            );
            if items.is_empty() {
                Ok(None)
            } else {
                Ok(Some(CompletionResponse::Array(items)))
            }
        } else {
            Ok(None)
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri.to_string();

        // Get document content from cache
        if let Some(content) = self.documents.get(&uri) {
            let symbols = document_symbols(&content);
            if symbols.is_empty() {
                Ok(None)
            } else {
                Ok(Some(DocumentSymbolResponse::Nested(symbols)))
            }
        } else {
            Ok(None)
        }
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri.to_string();
        let position = params.text_document_position_params.position;

        // Get document content from cache
        if let Some(content) = self.documents.get(&uri) {
            // Get file path for resolution
            let file_path = Url::parse(&uri)
                .ok()
                .and_then(|u| u.to_file_path().ok())
                .unwrap_or_default();

            let workspace_root = file_path.parent();

            Ok(get_path_definition(&content, position, &file_path, workspace_root))
        } else {
            Ok(None)
        }
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri.to_string();

        // Get document content from cache
        if let Some(content) = self.documents.get(&uri) {
            let tokens = compute_semantic_tokens(&content);
            Ok(Some(SemanticTokensResult::Tokens(tokens)))
        } else {
            Ok(None)
        }
    }
}
