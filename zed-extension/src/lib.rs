//! Zed extension for Fleet GitOps YAML validation.
//!
//! This extension integrates the fleet-schema-gen LSP server with Zed,
//! providing validation, completions, and diagnostics for Fleet configuration files.

use std::fs;
use zed::LanguageServerId;
use zed_extension_api::{self as zed, settings::LspSettings, Result};

/// The Fleet GitOps extension for Zed.
struct FleetExtension {
    /// Cached path to the fleet-schema-gen binary.
    cached_binary_path: Option<String>,
}

/// Binary name.
const BINARY_NAME: &str = "fleet-schema-gen";

impl FleetExtension {
    /// Try to find the binary in common locations.
    fn find_binary_in_common_paths(&self) -> Option<String> {
        // Common paths where the binary might be installed
        let common_paths = [
            // Cargo install location
            format!(
                "{}/.cargo/bin/{}",
                std::env::var("HOME").unwrap_or_default(),
                BINARY_NAME
            ),
            // Homebrew on macOS ARM
            format!("/opt/homebrew/bin/{}", BINARY_NAME),
            // Homebrew on macOS Intel
            format!("/usr/local/bin/{}", BINARY_NAME),
            // Linux standard paths
            format!("/usr/bin/{}", BINARY_NAME),
            format!("/usr/local/bin/{}", BINARY_NAME),
        ];

        for path in common_paths {
            if fs::metadata(&path).is_ok() {
                return Some(path);
            }
        }

        None
    }

    /// Get the binary path, trying multiple methods.
    fn get_binary_path(&mut self, worktree: &zed::Worktree) -> Result<String> {
        // 1. Return cached path if available and still exists
        if let Some(ref path) = self.cached_binary_path {
            if fs::metadata(path).is_ok() {
                return Ok(path.clone());
            }
        }

        // 2. Try worktree.which() (searches PATH)
        if let Some(path) = worktree.which(BINARY_NAME) {
            self.cached_binary_path = Some(path.clone());
            return Ok(path);
        }

        // 3. Try common installation paths
        if let Some(path) = self.find_binary_in_common_paths() {
            self.cached_binary_path = Some(path.clone());
            return Ok(path);
        }

        // 4. Binary not found - return helpful error
        // Note: Auto-download from GitHub is disabled for now since
        // fleet-schema-gen isn't published to GitHub releases yet.
        Err(format!(
            "fleet-schema-gen not found. Please install it:\n\
             cargo install --git https://github.com/fleetdm/fleet --path fleet-schema-gen\n\
             Or build locally: cd fleet-schema-gen && cargo install --path ."
        )
        .into())
    }
}

impl zed::Extension for FleetExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let binary_path = self.get_binary_path(worktree)?;

        Ok(zed::Command {
            command: binary_path,
            args: vec!["lsp".into()],
            env: vec![],
        })
    }

    fn language_server_initialization_options(
        &mut self,
        server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        let settings = LspSettings::for_worktree(server_id.as_ref(), worktree)
            .ok()
            .and_then(|lsp_settings| lsp_settings.initialization_options.clone())
            .unwrap_or_default();

        Ok(Some(settings))
    }

    fn language_server_workspace_configuration(
        &mut self,
        server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        let settings = LspSettings::for_worktree(server_id.as_ref(), worktree)
            .ok()
            .and_then(|lsp_settings| lsp_settings.settings.clone())
            .unwrap_or_default();

        Ok(Some(settings))
    }
}

zed::register_extension!(FleetExtension);
