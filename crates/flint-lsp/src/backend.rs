//! LSP backend implementation for Fleet GitOps validation.

use dashmap::DashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeActionParams, CodeActionProviderCapability, CodeActionResponse, CompletionOptions,
    CompletionParams, CompletionResponse, Diagnostic, DiagnosticSeverity,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, DocumentLink, DocumentLinkParams, DocumentSymbolParams,
    DocumentSymbolResponse, ExecuteCommandOptions, ExecuteCommandParams, FoldingRange,
    FoldingRangeParams, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams,
    HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams, MessageType,
    OneOf, Position, Range, SaveOptions, SemanticTokensFullOptions, SemanticTokensOptions,
    SemanticTokensParams, SemanticTokensResult, SemanticTokensServerCapabilities,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TextDocumentSyncSaveOptions, Url,
};
use tower_lsp::{Client, LanguageServer};

use super::code_actions::generate_code_actions;
use super::completion::complete_at_with_context;
use super::diagnostics::{gitops_error_to_diagnostic, lint_error_to_diagnostic};
use super::fleet::{
    find_gitops_root, FleetConnection, ResourceCache, SharedFleetConnection, SharedResourceCache,
};
use super::hover::hover_at_with_context;
use super::semantic_tokens::{compute_semantic_tokens, create_legend};
use super::symbols::document_symbols;
use super::workspace::{
    document_links, get_path_definition, validate_fma_slugs, validate_label_references,
    validate_path_references,
};
use flint_lint::fleet_config::Label;
use flint_lint::{FleetLintConfig, Linter};

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
    /// Fleet server connection for gitops validation (Layer 2).
    fleet_connection: SharedFleetConnection,
    /// Cached resource names from Fleet for live completions.
    resource_cache: SharedResourceCache,
    /// Whether gitops validation is enabled in config.
    gitops_enabled: RwLock<bool>,
    /// Whether live completions are enabled in config.
    live_completions_enabled: RwLock<bool>,
    /// Per-document timestamp of the latest `did_change` request.
    ///
    /// Drives keystroke debouncing: when a change handler completes its
    /// sleep window, it compares its own stored instant against this
    /// value. If they differ, a newer change has come in and this
    /// handler's lint is stale — it returns without linting. The
    /// debouncer is thus stateless beyond this single timestamp per URI.
    last_change_request: DashMap<String, std::time::Instant>,
}

impl FleetLspBackend {
    /// Create a new Fleet LSP backend.
    pub fn new(client: Client, linter: Linter) -> Self {
        Self {
            client,
            documents: DashMap::new(),
            linter: RwLock::new(linter),
            workspace_root: RwLock::new(None),
            fleet_connection: Arc::new(RwLock::new(None)),
            resource_cache: Arc::new(RwLock::new(None)),
            gitops_enabled: RwLock::new(false),
            live_completions_enabled: RwLock::new(false),
            last_change_request: DashMap::new(),
        }
    }

    /// Read the lint-debounce window from the active linter config.
    ///
    /// Falls back to the 150ms default if config isn't loaded or the lock
    /// is poisoned — debouncing must degrade gracefully, never deadlock or
    /// panic the LSP.
    fn lint_debounce_ms(&self) -> u32 {
        self.linter
            .read()
            .ok()
            .and_then(|linter| linter.config().map(|c| c.lsp.lint_debounce_ms))
            .unwrap_or(150)
    }

    /// Decide whether a debounced lint request should proceed.
    ///
    /// Pure helper, extracted so the coalesce-stale-requests logic is
    /// unit-testable without spinning up the LSP. Returns true iff the
    /// caller's own scheduled timestamp matches the latest recorded one
    /// for this URI — meaning no newer change has come in during the
    /// debounce sleep.
    fn debounce_decision(
        my_instant: std::time::Instant,
        latest: Option<std::time::Instant>,
    ) -> bool {
        matches!(latest, Some(t) if t == my_instant)
    }

    /// Load configuration from workspace root.
    fn load_config(&self, workspace_root: &Path) {
        if let Some((config_path, config)) = FleetLintConfig::find_and_load(workspace_root) {
            // Check Fleet connection settings before moving config
            let fleet_config = config.fleet.clone();

            // Update linter with new config
            if let Ok(mut linter) = self.linter.write() {
                linter.set_config(config);
            }

            // Log that we found a config
            let client = self.client.clone();
            let path = config_path.display().to_string();
            tokio::spawn(async move {
                client
                    .log_message(
                        MessageType::INFO,
                        format!("Loaded Fleet config from {}", path),
                    )
                    .await;
            });

            // Initialize Fleet connection if configured
            self.init_fleet_connection(&fleet_config);
        }
    }

    /// Initialize Fleet server connection from config + env vars.
    fn init_fleet_connection(&self, fleet_config: &flint_lint::FleetConnectionConfig) {
        // Store feature flags
        if let Ok(mut enabled) = self.gitops_enabled.write() {
            *enabled = fleet_config.gitops_validation;
        }
        if let Ok(mut enabled) = self.live_completions_enabled.write() {
            *enabled = fleet_config.live_completions;
        }

        // Only connect if at least one feature is enabled
        if !fleet_config.gitops_validation && !fleet_config.live_completions {
            return;
        }

        // Log resolution attempts
        {
            let client = self.client.clone();
            tokio::spawn(async move {
                client
                    .log_message(MessageType::INFO, "Fleet: resolving credentials...")
                    .await;
            });
        }

        let url =
            match fleet_config.resolved_url() {
                Some(url) => url,
                None => {
                    let client = self.client.clone();
                    tokio::spawn(async move {
                        client.log_message(
                        MessageType::WARNING,
                        "Fleet: no URL configured. Set `url` in [fleet] or FLEET_URL env var.",
                    ).await;
                    });
                    return;
                }
            };
        let token = match fleet_config.resolved_token() {
            Some(token) => token,
            None => {
                let client = self.client.clone();
                tokio::spawn(async move {
                    client.log_message(
                        MessageType::WARNING,
                        "Fleet: no token configured. Set `token` in [fleet] or FLEET_API_TOKEN env var.",
                    ).await;
                });
                return;
            }
        };

        let fleetctl_bin = fleet_config.resolved_fleetctl();

        // Build env vars for fleetctl: resolved URL, config env table, and existing FLEET_* vars.
        let mut extra_env = vec![("FLEET_URL".to_string(), url.clone())];

        // Resolve [fleet.env] entries (supports op:// references).
        for (k, v) in fleet_config.resolved_env() {
            if !extra_env.iter().any(|(ek, _)| ek == &k) {
                extra_env.push((k, v));
            }
        }

        // Also forward any FLEET_* env vars already set in the environment.
        for (k, v) in std::env::vars() {
            if k.starts_with("FLEET_") && !extra_env.iter().any(|(ek, _)| ek == &k) {
                extra_env.push((k, v));
            }
        }

        match FleetConnection::with_options(&url, &token, &fleetctl_bin, extra_env) {
            Ok(conn) => {
                // Populate initial resource cache if live completions enabled
                if fleet_config.live_completions {
                    let cache = conn.refresh_cache();
                    if let Ok(mut rc) = self.resource_cache.write() {
                        *rc = Some(cache);
                    }
                }

                if let Ok(mut fc) = self.fleet_connection.write() {
                    *fc = Some(conn);
                }

                let client = self.client.clone();
                let features: Vec<&str> = [
                    fleet_config
                        .gitops_validation
                        .then_some("gitops validation"),
                    fleet_config.live_completions.then_some("live completions"),
                ]
                .into_iter()
                .flatten()
                .collect();
                let features_str = features.join(", ");

                tokio::spawn(async move {
                    client
                        .log_message(
                            MessageType::INFO,
                            format!("Fleet server connected ({})", features_str),
                        )
                        .await;
                });
            }
            Err(e) => {
                let client = self.client.clone();
                let msg = format!("Fleet connection failed: {e}");
                tokio::spawn(async move {
                    client.log_message(MessageType::WARNING, msg).await;
                });
            }
        }
    }

    /// Handle document change - lint and publish diagnostics (Layer 1).
    ///
    /// Splits into a synchronous cache update plus an async lint+publish so
    /// the debouncer in `did_change` can update the cache immediately (so
    /// subsequent edits see fresh content) while deferring the expensive
    /// lint+publish.
    async fn on_change(&self, uri: String, content: String) {
        self.cache_document(uri.clone(), content);
        self.lint_and_publish(&uri).await;
    }

    /// Insert the latest content for `uri` into the document cache.
    /// Synchronous, idempotent, safe to call on every keystroke.
    fn cache_document(&self, uri: String, content: String) {
        self.documents.insert(uri, content);
    }

    /// Lint the cached content for `uri` and publish diagnostics.
    /// Used by `on_change` for the open/save paths, and by the debounced
    /// did_change path after the sleep window completes. Returns silently
    /// if the document is no longer in the cache (closed mid-flight).
    async fn lint_and_publish(&self, uri: &str) {
        let content = match self.documents.get(uri) {
            Some(entry) => entry.value().clone(),
            None => return,
        };
        let diagnostics = self.lint_document(uri, &content);
        if let Ok(url) = Url::parse(uri) {
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
        let linter = self.linter.read().expect("linter lock poisoned");
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
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: 0,
                            character: 0,
                        },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("fleet-lint".to_string()),
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

        // Add label reference validation (check labels_include_any/exclude_any
        // against labels defined in the workspace)
        if let Some(gitops_file) = super::fleet::find_gitops_root(&file_path_buf) {
            if let Some(root) = gitops_file.parent() {
                let known_labels = scan_workspace_label_names(root);
                if !known_labels.is_empty() {
                    diagnostics.extend(validate_label_references(content, &known_labels));
                }
            }
        }

        // Add FMA slug validation (check slug: values against the registry)
        diagnostics.extend(validate_fma_slugs(content));

        diagnostics
    }

    /// Run gitops dry-run validation asynchronously (Layer 2).
    ///
    /// Called on save. Publishes diagnostics with source "fleet-gitops".
    async fn run_gitops_validation(&self, uri: &str) {
        let gitops_enabled = self.gitops_enabled.read().ok().map(|e| *e).unwrap_or(false);
        if !gitops_enabled {
            return;
        }

        let file_path = match Url::parse(uri).ok().and_then(|u| u.to_file_path().ok()) {
            Some(p) => p,
            None => return,
        };

        // Find the gitops root file for validation
        let gitops_file = match find_gitops_root(&file_path) {
            Some(f) => f,
            None => {
                self.client
                    .log_message(
                        MessageType::LOG,
                        format!("Fleet: no gitops root found for {}", file_path.display()),
                    )
                    .await;
                return;
            }
        };

        let conn = Arc::clone(&self.fleet_connection);
        let client = self.client.clone();
        let uri = uri.to_string();
        let gitops_display = gitops_file.display().to_string();

        tokio::spawn(async move {
            // Log which file is being validated
            client
                .log_message(
                    MessageType::INFO,
                    format!("Fleet: dry-run validating {}", gitops_display),
                )
                .await;

            // Run dry-run in blocking thread (fleetctl is synchronous)
            let report = tokio::task::spawn_blocking(move || {
                let conn = conn.read().ok()?;
                let conn = conn.as_ref()?;
                Some(conn.gitops_dry_run(&gitops_file))
            })
            .await;

            let report = match report {
                Ok(Some(Ok(report))) => report,
                Ok(Some(Err(e))) => {
                    let msg = format!("Fleet: dry-run failed — {e}");
                    client.log_message(MessageType::ERROR, &msg).await;
                    client.show_message(MessageType::ERROR, &msg).await;
                    return;
                }
                Ok(None) => {
                    client
                        .log_message(
                            MessageType::WARNING,
                            "Fleet: no connection available for dry-run",
                        )
                        .await;
                    return;
                }
                Err(e) => {
                    client
                        .log_message(
                            MessageType::ERROR,
                            format!("Fleet: dry-run task panicked — {e}"),
                        )
                        .await;
                    return;
                }
            };

            // Log full output for debugging (including noise)
            for err in &report.errors {
                let prefix = if err.noise {
                    "  gitops (ignored): "
                } else {
                    "  gitops: "
                };
                client
                    .log_message(MessageType::LOG, format!("{}{}", prefix, err.message))
                    .await;
            }

            // Only real errors become diagnostics
            let real_errors: Vec<_> = report.errors.iter().filter(|e| !e.noise).collect();
            let diagnostics: Vec<Diagnostic> = real_errors
                .iter()
                .map(|e| gitops_error_to_diagnostic(e))
                .collect();

            // Show result to user as a popup + log
            if report.success {
                let msg = format!("Fleet: dry-run passed ({})", report.summary);
                client.log_message(MessageType::INFO, &msg).await;
                client.show_message(MessageType::INFO, msg).await;
            } else {
                // Include error messages + hints in the popup (truncate if too many)
                let error_lines: Vec<String> = real_errors
                    .iter()
                    .take(3)
                    .map(|e| {
                        if let Some(hint) = &e.hint {
                            format!("{}\n  → {}", e.message, hint)
                        } else {
                            e.message.clone()
                        }
                    })
                    .collect();
                let error_detail = error_lines.join("\n");
                let suffix = if real_errors.len() > 3 {
                    format!("\n...and {} more", real_errors.len() - 3)
                } else {
                    String::new()
                };
                let msg = format!("Fleet: dry-run failed\n{}{}", error_detail, suffix);
                client.log_message(MessageType::WARNING, &msg).await;
                client.show_message(MessageType::WARNING, msg).await;
            }

            // Publish gitops diagnostics on the saved file
            if let Ok(url) = Url::parse(&uri) {
                client.publish_diagnostics(url, diagnostics, None).await;
            }
        });
    }

    /// Get current resource cache snapshot for completions.
    pub fn get_resource_cache(&self) -> Option<ResourceCache> {
        let enabled = self
            .live_completions_enabled
            .read()
            .ok()
            .map(|e| *e)
            .unwrap_or(false);
        if !enabled {
            return None;
        }

        let cache = self.resource_cache.read().ok()?;
        let cache = cache.as_ref()?;

        // If stale, trigger background refresh
        if cache.is_stale() {
            let conn = Arc::clone(&self.fleet_connection);
            let cache_ref = Arc::clone(&self.resource_cache);
            tokio::spawn(async move {
                let new_cache = tokio::task::spawn_blocking(move || {
                    let conn = conn.read().ok()?;
                    let conn = conn.as_ref()?;
                    Some(conn.refresh_cache())
                })
                .await;

                if let Ok(Some(new_cache)) = new_cache {
                    if let Ok(mut rc) = cache_ref.write() {
                        *rc = Some(new_cache);
                    }
                }
            });
        }

        // Return current (possibly stale) data immediately
        Some(cache.clone())
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for FleetLspBackend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Extract editor-provided Fleet version from initialization options
        let editor_fleet_version = params
            .initialization_options
            .as_ref()
            .and_then(|opts| opts.get("fleetVersion"))
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty() && *v != "latest")
            .map(|v| v.to_string());

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

        // Override fleet_version from editor setting if provided (takes priority over .fleetlint.toml)
        if let Some(version) = &editor_fleet_version {
            if let Ok(mut linter) = self.linter.write() {
                if let Some(config) = linter.config_mut() {
                    config.deprecations.fleet_version = version.clone();
                }
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(false),
                        })),
                        ..Default::default()
                    },
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
                        "/".to_string(),
                        ".".to_string(),
                    ]),
                    ..Default::default()
                }),
                // Enable document symbols for outline view
                document_symbol_provider: Some(OneOf::Left(true)),
                // Enable go-to-definition for path references
                definition_provider: Some(OneOf::Left(true)),
                // Enable clickable document links for path references
                document_link_provider: Some(tower_lsp::lsp_types::DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: Default::default(),
                }),
                // Enable folding ranges for YAML sections
                folding_range_provider: Some(
                    tower_lsp::lsp_types::FoldingRangeProviderCapability::Simple(true),
                ),
                // Enable workspace commands (scaffold, etc.)
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["fleet.scaffold".to_string()],
                    ..Default::default()
                }),
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

        // Check if workspace has Fleet files; if not, suggest getting started
        let needs_scaffold = {
            let root = self.workspace_root.read().ok().and_then(|r| r.clone());
            if let Some(root_path) = root {
                let has_default = root_path.join("default.yml").exists()
                    || root_path.join("default.yaml").exists();
                let has_fleets = root_path.join("fleets").is_dir();
                let has_teams = root_path.join("teams").is_dir();
                !has_default && !has_fleets && !has_teams
            } else {
                false
            }
        };
        if needs_scaffold {
            self.client
                .show_message(
                    MessageType::INFO,
                    "No Fleet GitOps files detected. Run \"Fleet: Get Started\" to scaffold a new GitOps repository.",
                )
                .await;
        }
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        match params.command.as_str() {
            "fleet.scaffold" => {
                let root = self.workspace_root.read().ok().and_then(|r| r.clone());
                if let Some(root_path) = root {
                    match scaffold_gitops_repo(&root_path) {
                        Ok(files_created) => {
                            self.client
                                .show_message(
                                    MessageType::INFO,
                                    format!(
                                        "Fleet GitOps repository scaffolded! Created {} files. Open default.yml to get started.",
                                        files_created
                                    ),
                                )
                                .await;
                            Ok(Some(
                                serde_json::json!({ "success": true, "files_created": files_created }),
                            ))
                        }
                        Err(e) => {
                            self.client
                                .show_message(
                                    MessageType::ERROR,
                                    format!("Failed to scaffold: {}", e),
                                )
                                .await;
                            Ok(Some(
                                serde_json::json!({ "success": false, "error": e.to_string() }),
                            ))
                        }
                    }
                } else {
                    self.client
                        .show_message(MessageType::ERROR, "No workspace folder open.")
                        .await;
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let content = params.text_document.text;
        self.on_change(uri, content).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        // We request FULL sync, so there's always exactly one change with full content
        let Some(change) = params.content_changes.into_iter().next() else {
            return;
        };

        // Cache the content synchronously so subsequent edits (and any
        // in-flight lint task) see the latest text. This is cheap.
        self.cache_document(uri.clone(), change.text);

        let debounce_ms = self.lint_debounce_ms();
        if debounce_ms == 0 {
            // Debouncing disabled — preserve pre-debounce behavior of
            // linting on every keystroke. Used by perf-debugging users
            // and by tests setting `lint_debounce_ms = 0`.
            self.lint_and_publish(&uri).await;
            return;
        }

        // Record this handler's own scheduled instant. Newer did_change
        // calls overwrite it before we wake up; we'll detect that and
        // skip the lint. No JoinHandle bookkeeping, no spawned tasks.
        let my_instant = std::time::Instant::now();
        self.last_change_request
            .insert(uri.clone(), my_instant);

        tokio::time::sleep(std::time::Duration::from_millis(debounce_ms as u64)).await;

        let still_latest = Self::debounce_decision(
            my_instant,
            self.last_change_request.get(&uri).map(|e| *e.value()),
        );
        if !still_latest {
            return;
        }

        self.lint_and_publish(&uri).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.to_string();

        // Invalidate any pending debounced lints by overwriting the
        // scheduled-instant. Then re-lint synchronously so the user
        // always sees fresh diagnostics on Ctrl+S, never the delayed
        // pending-window result.
        self.last_change_request
            .insert(uri.clone(), std::time::Instant::now());
        self.lint_and_publish(&uri).await;

        // Layer 2: run gitops validation asynchronously
        self.run_gitops_validation(&uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();

        // Invalidate any pending debounced lint so a stale lint doesn't
        // publish diagnostics against a closed document. (Pending lints
        // won't crash on closed docs — `lint_and_publish` no-ops when the
        // cache entry is missing — but skipping the work is cleaner.)
        self.last_change_request.remove(&uri);

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
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let position = params.text_document_position_params.position;

        // Get document content from cache
        if let Some(content) = self.documents.get(&uri) {
            // Route JSON files to the JSON hover provider
            if uri.ends_with(".json") {
                return Ok(super::json_hover::json_hover_at(&content, position, &uri));
            }

            let future_names = self
                .linter
                .read()
                .ok()
                .and_then(|l| l.config().map(|c| c.deprecations.future_names))
                .unwrap_or(false);
            Ok(hover_at_with_context(&content, position, future_names))
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
            let file_path = Url::parse(&uri).ok().and_then(|u| u.to_file_path().ok());

            // Get workspace root
            let workspace_root = self.workspace_root.read().ok().and_then(|r| r.clone());

            // Get live resource cache for Fleet completions
            let cache = self.get_resource_cache();

            // Read future_names flag from config
            let future_names = self
                .linter
                .read()
                .ok()
                .and_then(|l| l.config().map(|c| c.deprecations.future_names))
                .unwrap_or(false);

            let mut items = complete_at_with_context(
                &content,
                position,
                file_path.as_deref(),
                workspace_root.as_deref(),
                future_names,
            );

            // Inject live completions from Fleet server if available
            if let Some(cache) = cache {
                inject_live_completions(&content, position, &cache, &mut items);
            }

            // Inject workspace label names for labels_include/exclude contexts
            if let Some(ref root) = workspace_root {
                inject_workspace_label_completions(&content, position, root, &mut items);
            }

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
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let position = params.text_document_position_params.position;

        // Get document content from cache
        if let Some(content) = self.documents.get(&uri) {
            // Get file path for resolution
            let file_path = Url::parse(&uri)
                .ok()
                .and_then(|u| u.to_file_path().ok())
                .unwrap_or_default();

            let workspace_root = file_path.parent();

            Ok(get_path_definition(
                &content,
                position,
                &file_path,
                workspace_root,
            ))
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

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri = params.text_document.uri.to_string();

        if let Some(content) = self.documents.get(&uri) {
            let file_path = Url::parse(&uri)
                .ok()
                .and_then(|u| u.to_file_path().ok())
                .unwrap_or_default();

            let workspace_root = self.workspace_root.read().ok().and_then(|r| r.clone());
            let links = document_links(&content, &file_path, workspace_root.as_deref());

            if links.is_empty() {
                Ok(None)
            } else {
                Ok(Some(links))
            }
        } else {
            Ok(None)
        }
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri = params.text_document.uri.to_string();

        if let Some(content) = self.documents.get(&uri) {
            let symbols = document_symbols(&content);
            let mut ranges = Vec::new();

            for symbol in &symbols {
                // Fold top-level sections
                let start_line = symbol.range.start.line;
                let end_line = symbol.range.end.line;
                if end_line > start_line {
                    ranges.push(FoldingRange {
                        start_line,
                        start_character: None,
                        end_line,
                        end_character: None,
                        kind: Some(tower_lsp::lsp_types::FoldingRangeKind::Region),
                        collapsed_text: None,
                    });
                }

                // Fold children (individual policies, queries, etc.)
                if let Some(children) = &symbol.children {
                    for child in children {
                        let cs = child.range.start.line;
                        let ce = child.range.end.line;
                        if ce > cs {
                            ranges.push(FoldingRange {
                                start_line: cs,
                                start_character: None,
                                end_line: ce,
                                end_character: None,
                                kind: Some(tower_lsp::lsp_types::FoldingRangeKind::Region),
                                collapsed_text: None,
                            });
                        }
                    }
                }
            }

            if ranges.is_empty() {
                Ok(None)
            } else {
                Ok(Some(ranges))
            }
        } else {
            Ok(None)
        }
    }
}

/// Inject live resource completions from Fleet server cache.
///
/// Adds label, team, and query names to completions when the cursor is
/// in a relevant context (e.g., labels_include_any, team, etc.).
/// Scaffold a new Fleet GitOps repository structure.
///
/// Creates `default.yml`, `fleets/` with an example fleet, and `lib/` with
/// subdirectories for shared policies, queries, and labels.
///
/// Returns the number of files created.
fn scaffold_gitops_repo(root: &std::path::Path) -> std::result::Result<usize, std::io::Error> {
    let mut count = 0;

    // Helper: write file if it doesn't exist
    let write_if_new =
        |path: &std::path::Path, content: &str, count: &mut usize| -> std::io::Result<()> {
            if !path.exists() {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, content)?;
                *count += 1;
            }
            Ok(())
        };

    // Helper: create dir with .gitkeep
    let gitkeep = |dir: &std::path::Path, count: &mut usize| -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let gk = dir.join(".gitkeep");
        if !gk.exists() {
            std::fs::write(&gk, "")?;
            *count += 1;
        }
        Ok(())
    };

    // ── default.yml ──────────────────────────────────────────
    write_if_new(
        &root.join("default.yml"),
        r#"# default.yml — Global (org-level) configuration
# See: https://fleetdm.com/docs/configuration/yaml-files

org_settings:
  org_info:
    org_name: My organization
  server_settings:
    server_url: $FLEET_URL

controls:
  # macos_migration:
  #   enable: true
  #   mode: voluntary
  # windows_enabled_and_configured: true

labels:
  - paths: ./labels/*.yml
"#,
        &mut count,
    )?;

    // ── fleets/ ──────────────────────────────────────────────
    write_if_new(
        &root.join("fleets/workstations.yml"),
        r#"name: "Workstations"
controls:
  apple_settings:
    configuration_profiles:
      - paths: ../platforms/macos/declaration-profiles/*.json
      - paths: ../platforms/macos/configuration-profiles/*.mobileconfig
  windows_settings:
    configuration_profiles:
      - paths: ../platforms/windows/configuration-profiles/*.xml
  scripts:
    - paths: ../platforms/macos/scripts/*.sh
    - paths: ../platforms/windows/scripts/*.ps1
    - paths: ../platforms/linux/scripts/*.sh

reports:
  - paths: ../platforms/all/reports/*.yml
  - paths: ../platforms/macos/reports/*.yml
  - paths: ../platforms/windows/reports/*.yml
  - paths: ../platforms/linux/reports/*.yml

policies:
  - paths: ../platforms/macos/policies/*.yml
  - paths: ../platforms/windows/policies/*.yml
  - paths: ../platforms/linux/policies/*.yml

software:
  packages: []
  fleet_maintained_apps: []
  app_store_apps: []
"#,
        &mut count,
    )?;

    write_if_new(
        &root.join("fleets/personal-mobile-devices.yml"),
        r#"name: "Personal mobile devices"
controls:
  apple_settings:
    configuration_profiles: []
  android_settings:
    configuration_profiles: []
    certificates: []

software:
  app_store_apps: []
"#,
        &mut count,
    )?;

    // ── labels/ ──────────────────────────────────────────────
    write_if_new(&root.join("labels/apple-silicon-macos-hosts.yml"),
        "- name: Apple Silicon macOS hosts\n  description: macOS hosts on Apple Silicon architecture\n  query: SELECT 1 FROM os_version WHERE arch LIKE 'ARM%';\n  label_membership_type: dynamic\n  platform: darwin\n",
        &mut count)?;

    // ── platforms/ ───────────────────────────────────────────
    // Structure matches `fleetctl new` templates exactly.
    // Each platform has different subdirectories.
    let platform_subdirs: &[(&str, &[&str])] = &[
        ("all", &["icons", "policies", "reports"]),
        (
            "android",
            &["configuration-profiles", "managed-app-configurations"],
        ),
        ("ios", &["configuration-profiles", "declaration-profiles"]),
        (
            "ipados",
            &["configuration-profiles", "declaration-profiles"],
        ),
        ("linux", &["policies", "reports", "scripts", "software"]),
        (
            "macos",
            &[
                "commands",
                "configuration-profiles",
                "declaration-profiles",
                "enrollment-profiles",
                "policies",
                "reports",
                "scripts",
                "software",
            ],
        ),
        (
            "windows",
            &[
                "configuration-profiles",
                "policies",
                "reports",
                "scripts",
                "software",
            ],
        ),
    ];

    for (platform, subdirs) in platform_subdirs {
        for subdir in *subdirs {
            gitkeep(
                &root.join("platforms").join(platform).join(subdir),
                &mut count,
            )?;
        }
    }

    Ok(count)
}

fn inject_live_completions(
    source: &str,
    position: Position,
    cache: &ResourceCache,
    items: &mut Vec<tower_lsp::lsp_types::CompletionItem>,
) {
    use tower_lsp::lsp_types::{
        CompletionItem, CompletionItemKind, InsertTextFormat, InsertTextMode,
    };

    let line_idx = position.line as usize;
    let line = source.lines().nth(line_idx).unwrap_or("");
    let trimmed = line.trim();

    // Check if we're in a labels context
    if trimmed.starts_with("labels_include_any:")
        || trimmed.starts_with("labels_exclude_any:")
        || trimmed.starts_with("labels_include_all:")
        || trimmed.starts_with("- ")
    {
        // Check parent context for labels arrays
        let in_labels_context = is_in_labels_list_context(source, line_idx);
        if in_labels_context
            || trimmed.starts_with("labels_include_any:")
            || trimmed.starts_with("labels_exclude_any:")
            || trimmed.starts_with("labels_include_all:")
        {
            for label in &cache.labels {
                items.push(CompletionItem {
                    label: label.clone(),
                    kind: Some(CompletionItemKind::VALUE),
                    detail: Some("(from Fleet server)".to_string()),
                    ..Default::default()
                });
            }
        }
    }

    // Check if we're at a label array item start inside `labels:` section
    if is_at_label_array_item_start(source, line_idx) && !cache.label_details.is_empty() {
        for (idx, label) in cache.label_details.iter().enumerate() {
            let name = label.name.as_deref().unwrap_or("unnamed");
            let snippet = label_to_snippet(label);
            let detail = label
                .description
                .as_deref()
                .unwrap_or("Label from Fleet server");
            items.push(CompletionItem {
                label: format!("{} (label)", name),
                kind: Some(CompletionItemKind::SNIPPET),
                detail: Some(format!("(Fleet) {}", detail)),
                insert_text: Some(snippet),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                insert_text_mode: Some(InsertTextMode::ADJUST_INDENTATION),
                filter_text: Some(name.to_string()),
                sort_text: Some(format!("0_label_{:03}", idx)),
                ..Default::default()
            });
        }
    }

    // Check if we're after a team: key
    if trimmed.starts_with("team:") {
        for team in &cache.fleets {
            items.push(CompletionItem {
                label: team.clone(),
                kind: Some(CompletionItemKind::VALUE),
                detail: Some("(from Fleet server)".to_string()),
                ..Default::default()
            });
        }
    }

    // Check if we're in a reports (queries) list context with a name: field
    if (trimmed.starts_with("name:") || (trimmed.starts_with("- name:")))
        && is_in_queries_list_context(source, line_idx)
    {
        for query in &cache.reports {
            items.push(CompletionItem {
                label: query.clone(),
                kind: Some(CompletionItemKind::VALUE),
                detail: Some("(from Fleet server)".to_string()),
                ..Default::default()
            });
        }
    }
}

/// Inject label name completions from workspace label files.
///
/// Scans `lib/**/labels/*.yml` and `default.yml` label paths to find label
/// names defined in the workspace, offering them in `labels_include_any`,
/// `labels_exclude_any`, and `labels_include_all` contexts.
fn inject_workspace_label_completions(
    source: &str,
    position: Position,
    workspace_root: &std::path::Path,
    items: &mut Vec<tower_lsp::lsp_types::CompletionItem>,
) {
    use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

    let line_idx = position.line as usize;
    let line = source.lines().nth(line_idx).unwrap_or("");
    let trimmed = line.trim();

    // Only offer in labels filter contexts
    let in_context = trimmed.starts_with("labels_include_any:")
        || trimmed.starts_with("labels_exclude_any:")
        || trimmed.starts_with("labels_include_all:")
        || (trimmed.starts_with("- ") && is_in_labels_list_context(source, line_idx));

    if !in_context {
        return;
    }

    let names = scan_workspace_label_names(workspace_root);
    for name in names {
        // Skip if already provided by live completions
        if items.iter().any(|i| i.label == name) {
            continue;
        }
        items.push(CompletionItem {
            label: name,
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("(from workspace)".to_string()),
            ..Default::default()
        });
    }
}

/// Scan workspace for label names defined in YAML files.
///
/// Walks `lib/` looking for directories named `labels` and reads `.yml`/`.yaml`
/// files inside them for `name:` fields.
fn scan_workspace_label_names(workspace_root: &std::path::Path) -> Vec<String> {
    let mut names = Vec::new();

    // Scan lib/ (traditional location)
    let lib_dir = workspace_root.join("lib");
    if lib_dir.is_dir() {
        find_label_dirs(&lib_dir, &mut names);
    }

    // Scan top-level labels/ directory
    let labels_dir = workspace_root.join("labels");
    if labels_dir.is_dir() {
        scan_label_yamls_in_dir(&labels_dir, &mut names);
    }

    // Scan platforms/*/labels/ directories
    let platforms_dir = workspace_root.join("platforms");
    if platforms_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&platforms_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Check for labels/ under each platform
                    let plat_labels = path.join("labels");
                    if plat_labels.is_dir() {
                        scan_label_yamls_in_dir(&plat_labels, &mut names);
                    }
                    // Also check lib/labels/ under each platform
                    let plat_lib = path.join("lib");
                    if plat_lib.is_dir() {
                        find_label_dirs(&plat_lib, &mut names);
                    }
                }
            }
        }
    }

    // Scan default.yml and fleet files for inline labels
    for name in &["default.yml", "default.yaml"] {
        let path = workspace_root.join(name);
        if path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                extract_label_names(&content, &mut names);
            }
        }
    }

    names.sort();
    names.dedup();
    names
}

/// Scan all YAML files in a directory for label names.
fn scan_label_yamls_in_dir(dir: &std::path::Path, names: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str());
                if matches!(ext, Some("yml" | "yaml")) {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        extract_label_names(&content, names);
                    }
                }
            }
        }
    }
}

/// Recursively find `labels/` directories and extract names from YAML files inside.
fn find_label_dirs(dir: &std::path::Path, names: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().map(|n| n == "labels").unwrap_or(false) {
                // Read all yml/yaml files in this labels/ directory
                if let Ok(label_files) = std::fs::read_dir(&path) {
                    for lf in label_files.flatten() {
                        let lf_path = lf.path();
                        let ext = lf_path.extension().and_then(|e| e.to_str());
                        if matches!(ext, Some("yml" | "yaml")) {
                            if let Ok(content) = std::fs::read_to_string(&lf_path) {
                                extract_label_names(&content, names);
                            }
                        }
                    }
                }
            } else {
                find_label_dirs(&path, names);
            }
        }
    }
}

/// Extract label names from YAML content.
///
/// Handles both formats:
/// - Top-level `name: value`
/// - Array item `- name: value`
fn extract_label_names(content: &str, names: &mut Vec<String>) {
    for line in content.lines() {
        let trimmed = line.trim();
        let name_value = if let Some(rest) = trimmed.strip_prefix("- name:") {
            Some(rest)
        } else {
            trimmed.strip_prefix("name:")
        };
        if let Some(value) = name_value {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                names.push(value.to_string());
            }
        }
    }
}

/// Check if the cursor is at a new array item position inside a `labels:` section.
///
/// Returns true when the cursor line is an empty or partial `- ` inside a `labels:` array
/// (the top-level section, not `labels_include_any` etc.).
fn is_at_label_array_item_start(source: &str, line_idx: usize) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let line = match lines.get(line_idx) {
        Some(l) => l,
        None => return false,
    };
    let trimmed = line.trim();

    // Must be at an array item position: "- " or empty or just "-"
    if !trimmed.is_empty() && !trimmed.starts_with("- ") && trimmed != "-" {
        return false;
    }

    let current_indent = line.len() - line.trim_start().len();

    // Walk backwards to find the parent key
    for i in (0..line_idx).rev() {
        let prev = match lines.get(i) {
            Some(l) => l,
            None => continue,
        };
        let prev_trimmed = prev.trim();
        if prev_trimmed.is_empty() {
            continue;
        }

        let prev_indent = prev.len() - prev.trim_start().len();

        // A sibling array item — keep looking for the parent key
        if prev_trimmed.starts_with("- ") && prev_indent == current_indent {
            continue;
        }

        // Found a key at a lower indent level — check if it's `labels:`
        if prev_indent < current_indent && prev_trimmed.ends_with(':') {
            return prev_trimmed == "labels:";
        }

        // Found a key with value or unrelated line at same/lower indent — not in labels
        if prev_indent <= current_indent {
            return false;
        }
    }

    false
}

/// Escape special snippet characters (`$`, `}`, `\`) in a string literal.
fn escape_snippet(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('$', "\\$")
        .replace('}', "\\}")
}

/// Build an LSP snippet string for a label block from server data.
///
/// Produces a YAML block like:
/// ```yaml
/// name: Production
/// description: Production hosts
/// query: SELECT 1 FROM ...
/// platform: darwin
/// label_membership_type: dynamic
/// ```
///
/// Fields with values from the server are pre-filled as tab-stop defaults
/// so the user can tab through and adjust.
fn label_to_snippet(label: &Label) -> String {
    let name = label.name.as_deref().unwrap_or("${1:label_name}");
    let desc = label.description.as_deref().unwrap_or("");
    let query = label.query.as_deref().unwrap_or("SELECT 1;");
    let platform = label.platform.as_deref().unwrap_or("");
    let membership = label.label_membership_type.as_deref().unwrap_or("dynamic");

    let mut snippet = format!("name: ${{1:{}}}\n", escape_snippet(name));
    snippet.push_str(&format!("  description: ${{2:{}}}\n", escape_snippet(desc)));
    snippet.push_str(&format!("  query: ${{3:{}}}\n", escape_snippet(query)));
    snippet.push_str(&format!(
        "  platform: ${{4:{}}}\n",
        escape_snippet(platform)
    ));
    snippet.push_str(&format!(
        "  label_membership_type: ${{5:{}}}",
        escape_snippet(membership)
    ));

    snippet
}

/// Check if the cursor is inside a labels list (include_any, exclude_any, include_all).
fn is_in_labels_list_context(source: &str, line_idx: usize) -> bool {
    let lines: Vec<&str> = source.lines().collect();

    for i in (0..line_idx).rev() {
        let line = lines.get(i).unwrap_or(&"");
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("labels_include_any:")
            || trimmed.starts_with("labels_exclude_any:")
            || trimmed.starts_with("labels_include_all:")
        {
            return true;
        }

        // Hit a different key at same or lower indent — not in labels context
        if trimmed.ends_with(':') || (trimmed.contains(':') && !trimmed.starts_with('-')) {
            let current_indent = lines
                .get(line_idx)
                .map(|l| l.len() - l.trim_start().len())
                .unwrap_or(0);
            let check_indent = line.len() - line.trim_start().len();
            if check_indent <= current_indent {
                return false;
            }
        }
    }

    false
}

/// Check if the cursor is inside a `queries:` list context.
fn is_in_queries_list_context(source: &str, line_idx: usize) -> bool {
    let lines: Vec<&str> = source.lines().collect();

    for i in (0..line_idx).rev() {
        let line = lines.get(i).unwrap_or(&"");
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("queries:") {
            return true;
        }

        // Hit a different key at same or lower indent — not in queries context
        if trimmed.ends_with(':') || (trimmed.contains(':') && !trimmed.starts_with('-')) {
            let current_indent = lines
                .get(line_idx)
                .map(|l| l.len() - l.trim_start().len())
                .unwrap_or(0);
            let check_indent = line.len() - line.trim_start().len();
            if check_indent <= current_indent {
                return false;
            }
        }
    }

    false
}

#[cfg(test)]
mod context_tests {
    use super::*;

    #[test]
    fn labels_include_all_triggers_context() {
        let source = "controls:\n  scripts:\n    labels_include_all:\n      - ";
        assert!(is_in_labels_list_context(source, 3));
    }

    #[test]
    fn labels_include_any_triggers_context() {
        let source = "controls:\n  scripts:\n    labels_include_any:\n      - ";
        assert!(is_in_labels_list_context(source, 3));
    }

    #[test]
    fn labels_exclude_any_triggers_context() {
        let source = "controls:\n  scripts:\n    labels_exclude_any:\n      - ";
        assert!(is_in_labels_list_context(source, 3));
    }

    #[test]
    fn non_labels_key_does_not_trigger() {
        let source = "controls:\n  scripts:\n    some_other_key:\n      - ";
        assert!(!is_in_labels_list_context(source, 3));
    }

    #[test]
    fn queries_context_detects_parent() {
        let source = "queries:\n  - name: ";
        assert!(is_in_queries_list_context(source, 1));
    }

    #[test]
    fn queries_context_rejects_other_parent() {
        let source = "policies:\n  - name: ";
        assert!(!is_in_queries_list_context(source, 1));
    }

    #[test]
    fn inject_live_completions_adds_labels_for_include_all() {
        let source = "controls:\n  scripts:\n    labels_include_all:";
        let position = Position {
            line: 2,
            character: 25,
        };
        let cache = crate::fleet::ResourceCache {
            labels: vec!["Production".to_string(), "Staging".to_string()],
            ..Default::default()
        };
        let mut items = vec![];
        inject_live_completions(source, position, &cache, &mut items);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].label, "Production");
    }

    #[test]
    fn inject_live_completions_adds_queries_for_name() {
        let source = "queries:\n  - name: ";
        let position = Position {
            line: 1,
            character: 10,
        };
        let cache = crate::fleet::ResourceCache {
            reports: vec!["Disk encryption".to_string()],
            ..Default::default()
        };
        let mut items = vec![];
        inject_live_completions(source, position, &cache, &mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "Disk encryption");
    }

    // --- is_at_label_array_item_start tests ---

    #[test]
    fn label_array_item_start_detects_dash() {
        let source = "labels:\n  - ";
        assert!(is_at_label_array_item_start(source, 1));
    }

    #[test]
    fn label_array_item_start_detects_bare_dash() {
        let source = "labels:\n  -";
        assert!(is_at_label_array_item_start(source, 1));
    }

    #[test]
    fn label_array_item_start_with_sibling() {
        let source = "labels:\n  - name: Existing\n    query: SELECT 1;\n  - ";
        assert!(is_at_label_array_item_start(source, 3));
    }

    #[test]
    fn label_array_item_start_empty_line() {
        let source = "labels:\n  ";
        assert!(is_at_label_array_item_start(source, 1));
    }

    #[test]
    fn label_array_item_start_rejects_other_section() {
        let source = "queries:\n  - ";
        assert!(!is_at_label_array_item_start(source, 1));
    }

    #[test]
    fn label_array_item_start_rejects_policies() {
        let source = "policies:\n  - ";
        assert!(!is_at_label_array_item_start(source, 1));
    }

    #[test]
    fn label_array_item_start_rejects_nested_labels_filter() {
        // This is labels_include_any, not the top-level labels: section
        let source = "controls:\n  scripts:\n    labels_include_any:\n      - ";
        assert!(!is_at_label_array_item_start(source, 3));
    }

    // --- escape_snippet tests ---

    #[test]
    fn escape_snippet_no_special_chars() {
        assert_eq!(escape_snippet("hello world"), "hello world");
    }

    #[test]
    fn escape_snippet_dollar_sign() {
        assert_eq!(escape_snippet("cost is $5"), "cost is \\$5");
    }

    #[test]
    fn escape_snippet_braces_and_backslash() {
        assert_eq!(escape_snippet("a\\b}c"), "a\\\\b\\}c");
    }

    // --- label_to_snippet tests ---

    #[test]
    fn label_to_snippet_full_data() {
        let label = flint_lint::fleet_config::Label {
            name: Some("Production".to_string()),
            description: Some("Prod hosts".to_string()),
            query: Some("SELECT 1;".to_string()),
            platform: Some("darwin".to_string()),
            label_membership_type: Some("dynamic".to_string()),
            hosts: None,
        };
        let snippet = label_to_snippet(&label);
        assert!(snippet.contains("name: ${1:Production}"));
        assert!(snippet.contains("description: ${2:Prod hosts}"));
        assert!(snippet.contains("query: ${3:SELECT 1;}"));
        assert!(snippet.contains("platform: ${4:darwin}"));
        assert!(snippet.contains("label_membership_type: ${5:dynamic}"));
    }

    #[test]
    fn label_to_snippet_escapes_special_chars() {
        let label = flint_lint::fleet_config::Label {
            name: Some("Cost > $100".to_string()),
            description: None,
            query: Some("SELECT * FROM tbl WHERE cost > $VAR;".to_string()),
            platform: None,
            label_membership_type: None,
            hosts: None,
        };
        let snippet = label_to_snippet(&label);
        assert!(
            snippet.contains("\\$100"),
            "Dollar sign should be escaped: {}",
            snippet
        );
        assert!(
            snippet.contains("\\$VAR"),
            "Dollar sign in query should be escaped: {}",
            snippet
        );
    }

    #[test]
    fn label_to_snippet_defaults_for_missing_fields() {
        let label = flint_lint::fleet_config::Label {
            name: Some("Minimal".to_string()),
            description: None,
            query: None,
            platform: None,
            label_membership_type: None,
            hosts: None,
        };
        let snippet = label_to_snippet(&label);
        assert!(snippet.contains("name: ${1:Minimal}"));
        assert!(snippet.contains("query: ${3:SELECT 1;}"));
        assert!(snippet.contains("label_membership_type: ${5:dynamic}"));
    }

    // --- inject_live_completions with label block snippets ---

    #[test]
    fn inject_live_completions_adds_label_blocks_at_array_start() {
        let source = "labels:\n  - ";
        let position = Position {
            line: 1,
            character: 4,
        };
        let label = flint_lint::fleet_config::Label {
            name: Some("Production".to_string()),
            description: Some("Prod hosts".to_string()),
            query: Some("SELECT 1;".to_string()),
            platform: Some("darwin".to_string()),
            label_membership_type: Some("dynamic".to_string()),
            hosts: None,
        };
        let cache = crate::fleet::ResourceCache {
            labels: vec!["Production".to_string()],
            label_details: vec![label],
            ..Default::default()
        };
        let mut items = vec![];
        inject_live_completions(source, position, &cache, &mut items);

        // Should have at least one snippet completion
        let snippets: Vec<_> = items
            .iter()
            .filter(|i| i.label.contains("(label)"))
            .collect();
        assert_eq!(snippets.len(), 1);
        assert_eq!(snippets[0].label, "Production (label)");
        assert!(snippets[0]
            .insert_text
            .as_ref()
            .unwrap()
            .contains("name: ${1:Production}"));
        assert!(snippets[0].detail.as_ref().unwrap().contains("Prod hosts"));
    }

    #[test]
    fn inject_live_completions_no_label_blocks_outside_labels_section() {
        let source = "queries:\n  - ";
        let position = Position {
            line: 1,
            character: 4,
        };
        let label = flint_lint::fleet_config::Label {
            name: Some("Production".to_string()),
            description: None,
            query: None,
            platform: None,
            label_membership_type: None,
            hosts: None,
        };
        let cache = crate::fleet::ResourceCache {
            labels: vec!["Production".to_string()],
            label_details: vec![label],
            ..Default::default()
        };
        let mut items = vec![];
        inject_live_completions(source, position, &cache, &mut items);

        let snippets: Vec<_> = items
            .iter()
            .filter(|i| i.label.contains("(label)"))
            .collect();
        assert!(
            snippets.is_empty(),
            "Should not offer label blocks outside labels: section"
        );
    }
}

#[cfg(test)]
mod debounce_tests {
    //! Keystroke-debounce logic. The orchestration around tokio::sleep and
    //! the LSP `Client` is hard to mock cleanly, so we test the pure
    //! decision helper (`FleetLspBackend::debounce_decision`) and verify
    //! `DashMap`'s overwrite semantics — together those cover every
    //! branch the live handlers exercise.

    use super::FleetLspBackend;
    use dashmap::DashMap;
    use std::time::{Duration, Instant};

    /// Helper: produce two distinct instants. Sleeping 1ms is the smallest
    /// reliable way to guarantee distinct `Instant` values across two
    /// readings without relying on platform clock resolution.
    fn two_instants() -> (Instant, Instant) {
        let a = Instant::now();
        std::thread::sleep(Duration::from_millis(1));
        let b = Instant::now();
        assert_ne!(a, b);
        (a, b)
    }

    #[test]
    fn decision_proceeds_when_my_instant_is_still_latest() {
        // The handler stored its own instant and nothing newer overwrote
        // it during the sleep window — it should lint.
        let (mine, _other) = two_instants();
        assert!(FleetLspBackend::debounce_decision(mine, Some(mine)));
    }

    #[test]
    fn decision_skips_when_newer_request_overwrote_mine() {
        // The handler woke up to find a newer change had updated the
        // timestamp during its sleep — the newer handler will do the
        // lint, so this one must skip.
        let (mine, newer) = two_instants();
        assert!(!FleetLspBackend::debounce_decision(mine, Some(newer)));
    }

    #[test]
    fn decision_skips_when_no_record_exists() {
        // did_close removed the entry while we were sleeping. Skip
        // rather than lint diagnostics against a closed document.
        let mine = Instant::now();
        assert!(!FleetLspBackend::debounce_decision(mine, None));
    }

    #[test]
    fn dashmap_insert_overwrites_in_natural_order() {
        // The debouncer relies on DashMap's overwrite semantics: the last
        // `insert(key, value)` wins. This test pins that contract — if
        // DashMap's semantics ever change (e.g. to merge-by-key), the
        // debouncer breaks silently. Better to fail here.
        let map: DashMap<String, Instant> = DashMap::new();
        let first = Instant::now();
        std::thread::sleep(Duration::from_millis(1));
        let second = Instant::now();

        map.insert("uri".into(), first);
        map.insert("uri".into(), second);

        let stored = map.get("uri").map(|e| *e.value()).unwrap();
        assert_eq!(stored, second, "later insert must overwrite earlier");
        assert_ne!(stored, first);
    }

    #[test]
    fn coalesce_simulation_matches_expected_outcome() {
        // Simulate the full coalesce flow without touching tokio or the
        // LSP client: three handlers each record their instant in order;
        // only the third's instant survives in the map; only the third's
        // decision returns true. This is what makes rapid typing
        // collapse to one lint at the end of the burst.
        let map: DashMap<String, Instant> = DashMap::new();

        let t1 = Instant::now();
        std::thread::sleep(Duration::from_millis(1));
        let t2 = Instant::now();
        std::thread::sleep(Duration::from_millis(1));
        let t3 = Instant::now();

        // Each handler in turn writes its own timestamp.
        map.insert("uri".into(), t1);
        map.insert("uri".into(), t2);
        map.insert("uri".into(), t3);

        // After all handlers wake up, each evaluates the decision:
        let latest = map.get("uri").map(|e| *e.value());
        assert!(!FleetLspBackend::debounce_decision(t1, latest));
        assert!(!FleetLspBackend::debounce_decision(t2, latest));
        assert!(FleetLspBackend::debounce_decision(t3, latest));
    }
}
