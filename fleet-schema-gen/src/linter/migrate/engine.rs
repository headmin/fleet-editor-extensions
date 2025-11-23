use super::backup::Backup;
use super::diff::{DiffSet, FileDiff};
use super::resolver::PathResolver;
use super::transformations::{apply_changes, execute_field_delete, execute_field_move, execute_field_rename};
use super::types::{FileChange, Migration, MigrationPlan, MigrationStep, Transformation, Version};
use anyhow::{Context, Result};
use colored::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Migration execution engine
pub struct MigrationEngine {
    migrations: Vec<Migration>,
    resolver: PathResolver,
}

impl MigrationEngine {
    pub fn new() -> Self {
        Self {
            migrations: Vec::new(),
            resolver: PathResolver::new(),
        }
    }

    /// Load migrations from a list
    pub fn load_migrations(&mut self, migrations: Vec<Migration>) {
        self.migrations = migrations;
    }

    /// Load migrations from a TOML file
    pub fn load_migrations_from_file(&mut self, path: &Path) -> Result<()> {
        let migrations = super::loader::load_migrations_from_file(path)?;
        self.migrations = migrations;
        Ok(())
    }

    /// Get the latest supported version
    pub fn latest_version(&self) -> Version {
        self.migrations
            .iter()
            .map(|m| &m.to_version)
            .max()
            .cloned()
            .unwrap_or_else(|| Version::new(4, 74, 0))
    }

    /// Create a migration plan
    pub fn plan(&mut self, path: &Path, from: &Version, to: &Version) -> Result<MigrationPlan> {
        println!("{} Creating migration plan from {} to {}...",
            "→".blue().bold(),
            from.to_string().cyan(),
            to.to_string().cyan()
        );

        // Find applicable migrations
        let applicable = self.find_migrations_between(from, to);

        if applicable.is_empty() {
            return Err(anyhow::anyhow!(
                "No migration path found from {} to {}",
                from,
                to
            ));
        }

        println!("{} Found {} migration(s):", "✓".green(), applicable.len());
        for migration in &applicable {
            println!("  - {} ({})", migration.id.bold(), migration.description);
        }

        // Build list of affected files
        let affected_files = self.find_affected_files(path, &applicable)?;

        println!("{} Will affect {} file(s)", "→".blue(), affected_files.len());

        // Generate migration steps
        let steps = self.generate_steps(path, &applicable, &affected_files)?;

        let estimated_changes: usize = steps.iter().map(|s| s.changes.len()).sum();

        Ok(MigrationPlan {
            migrations: applicable,
            affected_files,
            steps,
            estimated_changes,
        })
    }

    /// Execute a migration plan
    pub fn execute(&mut self, plan: &MigrationPlan, dry_run: bool) -> Result<()> {
        if dry_run {
            println!("{} DRY RUN MODE - No files will be modified\n", "ℹ".blue().bold());
        }

        // Create backup if not dry run
        let backup = if !dry_run {
            println!("{} Creating backup...", "→".blue().bold());
            Some(Backup::create(&plan.affected_files, Path::new("."))?)
        } else {
            None
        };

        // Track diffs
        let mut diff_set = DiffSet::new();

        // Execute each step
        for (idx, step) in plan.steps.iter().enumerate() {
            println!("\n{} Step {}/{}: {}",
                "→".blue().bold(),
                idx + 1,
                plan.steps.len(),
                step.description.bold()
            );

            // Read original content
            let original_content = if step.file.exists() {
                fs::read_to_string(&step.file)?
            } else {
                String::new()
            };

            // Apply changes
            let new_content = apply_changes(&step.file, &step.changes)?;

            // Create diff
            let diff = FileDiff::new(
                step.file.display().to_string(),
                original_content.clone(),
                new_content.clone(),
            );

            println!("  {}", diff.summary());
            diff_set.add(diff);

            // Write if not dry run
            if !dry_run {
                fs::write(&step.file, &new_content)
                    .with_context(|| format!("Failed to write {}", step.file.display()))?;
            }
        }

        // Show diff summary
        println!("\n{}", "=".repeat(60));
        diff_set.print_summary();

        if dry_run {
            println!("\n{} This was a dry run. No files were modified.", "ℹ".blue().bold());
        } else {
            println!("\n{} Migration completed successfully!", "✓".green().bold());

            if let Some(backup) = backup {
                println!("{} Backup saved at: {}",
                    "ℹ".blue(),
                    backup.backup_dir.display().to_string().dimmed()
                );
            }
        }

        Ok(())
    }

    // Helper methods

    fn find_migrations_between(&self, from: &Version, to: &Version) -> Vec<Migration> {
        self.migrations
            .iter()
            .filter(|m| &m.from_version >= from && &m.to_version <= to)
            .cloned()
            .collect()
    }

    fn find_affected_files(&mut self, root: &Path, migrations: &[Migration]) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        // For now, find all YAML files in the directory
        self.find_yaml_files_recursive(root, &mut files)?;

        Ok(files)
    }

    fn find_yaml_files_recursive(&self, dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Skip hidden directories
                if let Some(name) = path.file_name() {
                    if name.to_string_lossy().starts_with('.') {
                        continue;
                    }
                }
                self.find_yaml_files_recursive(&path, files)?;
            } else if let Some(ext) = path.extension() {
                if ext == "yml" || ext == "yaml" {
                    files.push(path);
                }
            }
        }

        Ok(())
    }

    /// Simple glob pattern matching (supports ** and * wildcards)
    fn matches_pattern(&self, path: &Path, pattern: &str) -> bool {
        let path_str = path.to_string_lossy();
        let pattern_parts: Vec<&str> = pattern.split('/').collect();
        let path_parts: Vec<&str> = path_str.split('/').collect();

        self.match_parts(&path_parts, &pattern_parts)
    }

    fn match_parts(&self, path_parts: &[&str], pattern_parts: &[&str]) -> bool {
        if pattern_parts.is_empty() {
            return path_parts.is_empty();
        }

        if pattern_parts[0] == "**" {
            // ** matches zero or more path segments
            if pattern_parts.len() == 1 {
                return true;
            }
            // Try matching rest of pattern at each position
            for i in 0..=path_parts.len() {
                if self.match_parts(&path_parts[i..], &pattern_parts[1..]) {
                    return true;
                }
            }
            false
        } else if path_parts.is_empty() {
            false
        } else if self.matches_segment(path_parts[0], pattern_parts[0]) {
            self.match_parts(&path_parts[1..], &pattern_parts[1..])
        } else {
            false
        }
    }

    fn matches_segment(&self, path_seg: &str, pattern_seg: &str) -> bool {
        if pattern_seg == "*" {
            return true;
        }

        // Simple * wildcard matching
        if pattern_seg.contains('*') {
            let parts: Vec<&str> = pattern_seg.split('*').collect();
            let mut pos = 0;
            for (i, part) in parts.iter().enumerate() {
                if part.is_empty() {
                    continue;
                }
                if i == 0 && !path_seg.starts_with(part) {
                    return false;
                }
                if let Some(idx) = path_seg[pos..].find(part) {
                    pos += idx + part.len();
                } else {
                    return false;
                }
            }
            if !parts.is_empty() && !parts.last().unwrap().is_empty() && !path_seg.ends_with(parts.last().unwrap()) {
                return false;
            }
            true
        } else {
            path_seg == pattern_seg
        }
    }

    fn generate_steps(
        &mut self,
        _root: &Path,
        migrations: &[Migration],
        affected_files: &[PathBuf],
    ) -> Result<Vec<MigrationStep>> {
        let mut steps = Vec::new();

        for migration in migrations {
            for transformation in &migration.transformations {
                match transformation {
                    Transformation::FieldMove {
                        source_pattern,
                        target_pattern,
                        fields,
                        match_strategy,
                        target_location,
                    } => {
                        // For PathReference strategy: team files reference software files
                        if matches!(match_strategy, super::types::MatchStrategy::PathReference) {
                            // Find target files (e.g., teams/**/*.yml)
                            for file in affected_files {
                                if self.matches_pattern(file, target_pattern) {
                                    // Find referenced software files
                                    if let Ok(referenced) = self.resolver.find_referenced_files(file) {
                                        for software_file in referenced {
                                            if self.matches_pattern(&software_file, source_pattern) {
                                                // Load source file and extract fields
                                                if let Ok(source_yaml) = self.resolver.load_file(&software_file) {
                                                    let mut source_changes = Vec::new();
                                                    let mut target_changes = Vec::new();

                                                    for field in fields {
                                                        if let Some(value) = source_yaml.get(field) {
                                                            // Remove from source
                                                            source_changes.push(FileChange::RemoveField {
                                                                path: field.clone(),
                                                            });

                                                            // Add to target at specified location
                                                            target_changes.push(FileChange::AddField {
                                                                path: format!("{}.{}", target_location, field),
                                                                value: value.clone(),
                                                            });
                                                        }
                                                    }

                                                    // Create step for source file
                                                    if !source_changes.is_empty() {
                                                        steps.push(MigrationStep {
                                                            description: format!(
                                                                "Remove team-specific fields from {}",
                                                                software_file.display()
                                                            ),
                                                            file: software_file.clone(),
                                                            changes: source_changes,
                                                        });
                                                    }

                                                    // Create step for target file
                                                    if !target_changes.is_empty() {
                                                        steps.push(MigrationStep {
                                                            description: format!(
                                                                "Add software package settings to {}",
                                                                file.display()
                                                            ),
                                                            file: file.clone(),
                                                            changes: target_changes,
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Transformation::FieldRename { .. } => {
                        for file in affected_files {
                            if let Ok(changes) = execute_field_rename(transformation, file, &mut self.resolver) {
                                if !changes.is_empty() {
                                    steps.push(MigrationStep {
                                        description: format!("Rename fields in {}", file.display()),
                                        file: file.clone(),
                                        changes,
                                    });
                                }
                            }
                        }
                    }
                    Transformation::FieldDelete { .. } => {
                        for file in affected_files {
                            if let Ok(changes) = execute_field_delete(transformation, file, &mut self.resolver) {
                                if !changes.is_empty() {
                                    steps.push(MigrationStep {
                                        description: format!("Delete deprecated fields in {}", file.display()),
                                        file: file.clone(),
                                        changes,
                                    });
                                }
                            }
                        }
                    }
                    Transformation::Restructure { name, description } => {
                        // Custom restructuring logic would go here
                        println!("{} Restructure transformation '{}' not yet implemented",
                            "⚠".yellow(),
                            name
                        );
                    }
                }
            }
        }

        Ok(steps)
    }
}

impl Default for MigrationEngine {
    fn default() -> Self {
        Self::new()
    }
}

