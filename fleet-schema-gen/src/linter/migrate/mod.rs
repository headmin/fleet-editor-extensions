pub mod types;
pub mod detector;
pub mod resolver;
pub mod engine;
pub mod transformations;
pub mod backup;
pub mod git;
pub mod diff;
pub mod loader;

pub use types::{Migration, Transformation, MigrationPlan, Version};
pub use detector::VersionDetector;
pub use resolver::PathResolver;
pub use engine::MigrationEngine;
pub use backup::Backup;

use anyhow::Result;
use std::path::Path;

/// Main entry point for migrations
pub struct Migrator {
    detector: VersionDetector,
    engine: MigrationEngine,
}

impl Migrator {
    pub fn new() -> Self {
        Self {
            detector: VersionDetector::new(),
            engine: MigrationEngine::new(),
        }
    }

    /// Detect the Fleet version of a configuration
    pub fn detect_version(&self, path: &Path) -> Result<Option<Version>> {
        self.detector.detect(path)
    }

    /// Load migrations from a TOML file
    pub fn load_migrations_from_file(&mut self, path: &Path) -> Result<()> {
        self.engine.load_migrations_from_file(path)
    }

    /// Get the latest supported version
    pub fn latest_version(&self) -> Version {
        self.engine.latest_version()
    }

    /// Create a migration plan
    pub fn plan_migration(
        &mut self,
        path: &Path,
        from: &Version,
        to: &Version,
    ) -> Result<MigrationPlan> {
        self.engine.plan(path, from, to)
    }

    /// Execute a migration
    pub fn execute_migration(
        &mut self,
        plan: &MigrationPlan,
        dry_run: bool,
    ) -> Result<()> {
        self.engine.execute(plan, dry_run)
    }

    /// Auto-migrate to latest version
    pub fn auto_migrate(&mut self, path: &Path, dry_run: bool) -> Result<()> {
        // Detect current version
        let current = self.detect_version(path)?
            .ok_or_else(|| anyhow::anyhow!("Could not detect Fleet version"))?;

        // Get latest supported version
        let latest = self.engine.latest_version();

        if current >= latest {
            println!("Already at latest version: {}", current);
            return Ok(());
        }

        // Create plan
        let plan = self.plan_migration(path, &current, &latest)?;

        // Execute
        self.execute_migration(&plan, dry_run)
    }
}

impl Default for Migrator {
    fn default() -> Self {
        Self::new()
    }
}
