//! Zed extension for Fleet GitOps YAML validation.
//!
//! This extension integrates the fleet-schema-gen LSP server with Zed,
//! providing validation, completions, and diagnostics for Fleet configuration files.
//!
//! The extension will automatically download the LSP binary from GitHub releases
//! if it's not found in PATH or common installation locations.

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

/// GitHub repository for releases.
const GITHUB_REPO: &str = "fleetdm/fleet";

impl FleetExtension {
    /// Try to find the binary in common locations.
    fn find_binary_in_common_paths(&self) -> Option<String> {
        let common_paths = [
            // Cargo install location
            format!(
                "{}/.cargo/bin/{}",
                std::env::var("HOME").unwrap_or_default(),
                BINARY_NAME
            ),
            // Homebrew on macOS ARM
            format!("/opt/homebrew/bin/{}", BINARY_NAME),
            // Homebrew on macOS Intel / Linux
            format!("/usr/local/bin/{}", BINARY_NAME),
            // Linux standard paths
            format!("/usr/bin/{}", BINARY_NAME),
        ];

        for path in common_paths {
            if fs::metadata(&path).is_ok() {
                return Some(path);
            }
        }

        None
    }

    /// Get the platform-specific asset name for downloading.
    fn get_asset_name(version: &str) -> Option<String> {
        let (os, arch) = zed::current_platform();

        let platform = match (os, arch) {
            (zed::Os::Mac, zed::Architecture::Aarch64) => "darwin-arm64",
            (zed::Os::Mac, zed::Architecture::X8664) => "darwin-x64",
            (zed::Os::Linux, zed::Architecture::Aarch64) => "linux-arm64",
            (zed::Os::Linux, zed::Architecture::X8664) => "linux-x64",
            _ => return None,
        };

        Some(format!("{}-{}-{}.tar.gz", BINARY_NAME, version, platform))
    }

    /// Download and install the binary from GitHub releases.
    fn download_binary(&self) -> Result<String> {
        zed::set_language_server_installation_status(
            &LanguageServerId::new(BINARY_NAME),
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        // Get the latest release from GitHub
        let release = zed::latest_github_release(
            GITHUB_REPO,
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: true, // We use pre-releases for now
            },
        )?;

        // Extract version from tag (e.g., "v0.1.0" -> "0.1.0")
        let version = release.version.trim_start_matches('v');

        // Get the asset name for this platform
        let asset_name = Self::get_asset_name(version).ok_or_else(|| {
            format!(
                "Unsupported platform: {:?}",
                zed::current_platform()
            )
        })?;

        // Find the matching asset in the release
        let asset = release
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .ok_or_else(|| {
                format!(
                    "No matching asset found for platform. Expected: {}",
                    asset_name
                )
            })?;

        // Check if we already have this version
        let binary_path = format!("{}-{}", BINARY_NAME, version);
        if fs::metadata(&binary_path).is_ok() {
            // Already downloaded
            return Ok(binary_path);
        }

        zed::set_language_server_installation_status(
            &LanguageServerId::new(BINARY_NAME),
            &zed::LanguageServerInstallationStatus::Downloading,
        );

        // Download the archive
        let archive_path = format!("{}.tar.gz", binary_path);
        zed::download_file(
            &asset.download_url,
            &archive_path,
            zed::DownloadedFileType::GzipTar,
        )
        .map_err(|e| format!("Failed to download {}: {}", asset_name, e))?;

        // The archive extracts to the binary name
        let extracted_binary = BINARY_NAME.to_string();

        // Rename to versioned path
        fs::rename(&extracted_binary, &binary_path)
            .map_err(|e| format!("Failed to rename binary: {}", e))?;

        // Make executable
        zed::make_file_executable(&binary_path)?;

        // Clean up old versions (keep only the current one)
        if let Ok(entries) = fs::read_dir(".") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(BINARY_NAME) && name != binary_path && !name.ends_with(".tar.gz") {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }

        Ok(binary_path)
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

        // 4. Auto-download from GitHub releases
        let path = self.download_binary()?;
        self.cached_binary_path = Some(path.clone());
        Ok(path)
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
