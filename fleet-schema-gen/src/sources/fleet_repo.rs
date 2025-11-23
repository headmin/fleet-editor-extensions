use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const FLEET_REPO_URL: &str = "https://github.com/fleetdm/fleet.git";
const DEFAULT_FLEET_REPO_PATH: &str = "/tmp/fleet";

/// Manages Fleet repository cloning and updates
pub struct FleetRepo {
    repo_path: PathBuf,
}

impl FleetRepo {
    /// Create a new FleetRepo manager with default path
    pub fn new() -> Self {
        Self {
            repo_path: PathBuf::from(DEFAULT_FLEET_REPO_PATH),
        }
    }

    /// Create a FleetRepo manager with custom path
    pub fn with_path(path: PathBuf) -> Self {
        Self { repo_path: path }
    }

    /// Get the path to the Fleet repository
    pub fn path(&self) -> &Path {
        &self.repo_path
    }

    /// Ensure Fleet repository is available (clone if needed, update if exists)
    pub fn ensure_repo(&self, version: Option<&str>) -> Result<()> {
        if self.repo_path.exists() {
            println!("  → Fleet repository exists at: {}", self.repo_path.display());

            if let Some(ver) = version {
                self.checkout_version(ver)?;
            } else {
                self.update_repo()?;
            }
        } else {
            println!("  → Cloning Fleet repository...");
            self.clone_repo(version)?;
        }

        Ok(())
    }

    /// Clone the Fleet repository
    fn clone_repo(&self, version: Option<&str>) -> Result<()> {
        let mut cmd = Command::new("git");
        cmd.arg("clone");

        // If version is specified and looks like a tag, do a shallow clone
        if version.is_some() {
            cmd.arg("--depth").arg("1");
            if let Some(ver) = version {
                if ver.starts_with('v') || ver.starts_with("fleet-v") {
                    cmd.arg("--branch").arg(ver);
                }
            }
        }

        cmd.arg(FLEET_REPO_URL).arg(&self.repo_path);

        let output = cmd
            .output()
            .context("Failed to execute git clone command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Git clone failed: {}", stderr);
        }

        println!("  ✓ Cloned Fleet repository to {}", self.repo_path.display());

        // If version was specified but not as a branch, checkout after cloning
        if let Some(ver) = version {
            if !ver.starts_with('v') && !ver.starts_with("fleet-v") {
                self.checkout_version(ver)?;
            }
        }

        Ok(())
    }

    /// Update existing repository
    fn update_repo(&self) -> Result<()> {
        println!("  → Updating Fleet repository...");

        let output = Command::new("git")
            .arg("-C")
            .arg(&self.repo_path)
            .arg("pull")
            .arg("--quiet")
            .output()
            .context("Failed to execute git pull")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("  ⚠ Git pull failed: {}", stderr);
            eprintln!("  → Continuing with current repository state");
        } else {
            println!("  ✓ Updated Fleet repository");
        }

        Ok(())
    }

    /// Checkout a specific version (tag or commit)
    fn checkout_version(&self, version: &str) -> Result<()> {
        // Handle "latest" as a special case - just pull main
        if version == "latest" {
            self.update_repo()?;
            return Ok(());
        }

        println!("  → Checking out Fleet version: {}", version);

        let output = Command::new("git")
            .arg("-C")
            .arg(&self.repo_path)
            .arg("checkout")
            .arg(version)
            .output()
            .context("Failed to execute git checkout")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Git checkout failed: {}", stderr);
        }

        println!("  ✓ Checked out version: {}", version);
        Ok(())
    }

    /// Get the current HEAD commit hash
    pub fn get_current_version(&self) -> Result<String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.repo_path)
            .arg("rev-parse")
            .arg("--short")
            .arg("HEAD")
            .output()
            .context("Failed to get git commit hash")?;

        if !output.status.success() {
            anyhow::bail!("Failed to get current version");
        }

        let commit = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();

        Ok(commit)
    }

    /// Get the current tag if on a tag
    pub fn get_current_tag(&self) -> Result<Option<String>> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.repo_path)
            .arg("describe")
            .arg("--tags")
            .arg("--exact-match")
            .output()
            .context("Failed to get git tag")?;

        if !output.status.success() {
            // Not on a tag
            return Ok(None);
        }

        let tag = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();

        Ok(Some(tag))
    }

    /// List available tags (last N tags)
    pub fn list_tags(&self, limit: usize) -> Result<Vec<String>> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.repo_path)
            .arg("tag")
            .arg("-l")
            .arg("--sort=-v:refname")
            .output()
            .context("Failed to list git tags")?;

        if !output.status.success() {
            anyhow::bail!("Failed to list tags");
        }

        let tags: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .take(limit)
            .map(|s| s.to_string())
            .collect();

        Ok(tags)
    }
}

impl Default for FleetRepo {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fleet_repo_creation() {
        let repo = FleetRepo::new();
        assert_eq!(repo.path(), Path::new(DEFAULT_FLEET_REPO_PATH));
    }

    #[test]
    fn test_fleet_repo_custom_path() {
        let custom_path = PathBuf::from("/custom/path");
        let repo = FleetRepo::with_path(custom_path.clone());
        assert_eq!(repo.path(), &custom_path);
    }
}
