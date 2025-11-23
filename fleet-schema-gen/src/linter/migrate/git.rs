use anyhow::{Context, Result};
use colored::*;
use git2::{
    BranchType, Commit, IndexAddOption, ObjectType, Oid, Repository, Signature, StatusOptions,
};
use std::path::Path;

/// Git integration for migrations
pub struct GitMigrator {
    repo: Repository,
}

impl GitMigrator {
    /// Open a Git repository at the given path
    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repository::discover(path)
            .with_context(|| format!("Failed to find Git repository at {}", path.display()))?;

        Ok(Self { repo })
    }

    /// Check if we're in a Git repository
    pub fn is_git_repo(path: &Path) -> bool {
        Repository::discover(path).is_ok()
    }

    /// Get current branch name
    pub fn current_branch(&self) -> Result<String> {
        let head = self.repo.head()?;
        let branch_name = head
            .shorthand()
            .ok_or_else(|| anyhow::anyhow!("Could not get branch name"))?;

        Ok(branch_name.to_string())
    }

    /// Create a new branch for migration
    pub fn create_migration_branch(&self, from_version: &str, to_version: &str) -> Result<String> {
        let branch_name = format!("fleet-migrate-{}-to-{}", from_version, to_version);

        // Get current HEAD commit
        let head_commit = self.get_head_commit()?;

        // Check if branch already exists
        if self.repo.find_branch(&branch_name, BranchType::Local).is_ok() {
            return Err(anyhow::anyhow!(
                "Branch '{}' already exists. Delete it first or use a different name.",
                branch_name
            ));
        }

        // Create new branch
        self.repo.branch(&branch_name, &head_commit, false)?;

        // Switch to new branch
        self.checkout_branch(&branch_name)?;

        println!("{} Created and switched to branch: {}",
            "✓".green().bold(),
            branch_name.cyan().bold()
        );

        Ok(branch_name)
    }

    /// Checkout a branch
    pub fn checkout_branch(&self, branch_name: &str) -> Result<()> {
        let obj = self.repo.revparse_single(&format!("refs/heads/{}", branch_name))?;

        self.repo.checkout_tree(&obj, None)?;
        self.repo.set_head(&format!("refs/heads/{}", branch_name))?;

        Ok(())
    }

    /// Stage all changes
    pub fn stage_all(&self) -> Result<()> {
        let mut index = self.repo.index()?;
        index.add_all(["."].iter(), IndexAddOption::DEFAULT, None)?;
        index.write()?;

        Ok(())
    }

    /// Stage specific files
    pub fn stage_files(&self, files: &[&Path]) -> Result<()> {
        let mut index = self.repo.index()?;

        for file in files {
            index.add_path(file)?;
        }

        index.write()?;

        Ok(())
    }

    /// Commit staged changes
    pub fn commit(&self, message: &str) -> Result<Oid> {
        let mut index = self.repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;

        let parent_commit = self.get_head_commit()?;
        let signature = self.get_signature()?;

        let commit_id = self.repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &[&parent_commit],
        )?;

        println!("{} Committed: {}",
            "✓".green().bold(),
            message.bold()
        );

        Ok(commit_id)
    }

    /// Create a commit for migration
    pub fn commit_migration(
        &self,
        from_version: &str,
        to_version: &str,
        files_changed: usize,
    ) -> Result<Oid> {
        let message = format!(
            "Migrate Fleet GitOps from {} to {}\n\n\
            Automated migration of {} file(s).\n\
            \n\
            Migration performed by fleet-schema-gen",
            from_version, to_version, files_changed
        );

        self.commit(&message)
    }

    /// Show diff of uncommitted changes
    pub fn show_diff(&self) -> Result<String> {
        let diff = self.repo.diff_index_to_workdir(None, None)?;

        let mut output = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let content = String::from_utf8_lossy(line.content());
            let prefix = match line.origin() {
                '+' => "+",
                '-' => "-",
                ' ' => " ",
                _ => "",
            };
            output.push_str(&format!("{}{}", prefix, content));
            true
        })?;

        Ok(output)
    }

    /// Get status of working directory
    pub fn status(&self) -> Result<Vec<String>> {
        let mut status_opts = StatusOptions::new();
        status_opts.include_untracked(true);

        let statuses = self.repo.statuses(Some(&mut status_opts))?;

        let mut files = Vec::new();
        for entry in statuses.iter() {
            if let Some(path) = entry.path() {
                files.push(path.to_string());
            }
        }

        Ok(files)
    }

    /// Check if there are uncommitted changes
    pub fn has_uncommitted_changes(&self) -> Result<bool> {
        let statuses = self.status()?;
        Ok(!statuses.is_empty())
    }

    /// Check if current branch is clean (no uncommitted changes)
    pub fn is_clean(&self) -> Result<bool> {
        Ok(!self.has_uncommitted_changes()?)
    }

    /// Create a pull request (via gh CLI)
    pub fn create_pr(&self, title: &str, body: &str) -> Result<()> {
        // Check if gh CLI is available
        let gh_available = std::process::Command::new("gh")
            .arg("--version")
            .output()
            .is_ok();

        if !gh_available {
            println!("{} GitHub CLI (gh) not found. Skipping PR creation.",
                "⚠️".yellow()
            );
            println!("   Install with: brew install gh");
            return Ok(());
        }

        // Create PR using gh CLI
        let output = std::process::Command::new("gh")
            .args(&["pr", "create", "--title", title, "--body", body])
            .output()?;

        if output.status.success() {
            let pr_url = String::from_utf8_lossy(&output.stdout);
            println!("{} Pull request created: {}",
                "✓".green().bold(),
                pr_url.trim().cyan()
            );
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            println!("{} Failed to create PR: {}",
                "✗".red().bold(),
                error.trim()
            );
        }

        Ok(())
    }

    /// Push current branch to remote
    pub fn push(&self, branch: &str) -> Result<()> {
        println!("{} Pushing branch {} to remote...",
            "→".blue().bold(),
            branch.cyan()
        );

        // This is a simplified version - real implementation would need credentials
        let output = std::process::Command::new("git")
            .args(&["push", "-u", "origin", branch])
            .output()?;

        if output.status.success() {
            println!("{} Branch pushed successfully", "✓".green().bold());
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to push: {}", error));
        }

        Ok(())
    }

    // Helper methods

    fn get_head_commit(&self) -> Result<Commit> {
        let head = self.repo.head()?;
        let oid = head.target().ok_or_else(|| anyhow::anyhow!("HEAD has no target"))?;
        let commit = self.repo.find_commit(oid)?;
        Ok(commit)
    }

    fn get_signature(&self) -> Result<Signature> {
        // Try to get signature from config
        match self.repo.signature() {
            Ok(sig) => Ok(sig),
            Err(_) => {
                // Fallback to default signature
                Signature::now("Fleet Migration Bot", "noreply@fleet.com")
                    .context("Failed to create signature")
            }
        }
    }
}

/// Helper function to check if a path is in a Git repository
pub fn is_in_git_repo(path: &Path) -> bool {
    GitMigrator::is_git_repo(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_is_git_repo() {
        let temp = TempDir::new().unwrap();
        assert!(!GitMigrator::is_git_repo(temp.path()));

        // Initialize repo
        Repository::init(temp.path()).unwrap();
        assert!(GitMigrator::is_git_repo(temp.path()));
    }

    #[test]
    fn test_current_branch() {
        let temp = TempDir::new().unwrap();
        let repo = Repository::init(temp.path()).unwrap();

        // Create initial commit
        let sig = Signature::now("Test", "test@example.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();

        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();

        let git = GitMigrator::open(temp.path()).unwrap();
        let branch = git.current_branch().unwrap();

        assert_eq!(branch, "master".to_string());
    }

    #[test]
    fn test_create_migration_branch() {
        let temp = TempDir::new().unwrap();
        let repo = Repository::init(temp.path()).unwrap();

        // Create initial commit
        let sig = Signature::now("Test", "test@example.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();

        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();

        let git = GitMigrator::open(temp.path()).unwrap();
        let branch = git.create_migration_branch("4.73", "4.74").unwrap();

        assert_eq!(branch, "fleet-migrate-4.73-to-4.74");
        assert_eq!(git.current_branch().unwrap(), branch);
    }
}
