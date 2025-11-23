use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Backup of files before migration
#[derive(Debug, Clone)]
pub struct Backup {
    pub timestamp: DateTime<Utc>,
    pub backup_dir: PathBuf,
    pub files: HashMap<PathBuf, String>, // Original path -> content
}

impl Backup {
    /// Create a new backup of the given files
    pub fn create(files: &[PathBuf], base_dir: &Path) -> Result<Self> {
        let timestamp = Utc::now();
        let backup_dir = base_dir.join(format!(
            ".fleet-migration-backup-{}",
            timestamp.format("%Y%m%d-%H%M%S")
        ));

        fs::create_dir_all(&backup_dir)
            .with_context(|| format!("Failed to create backup directory: {}", backup_dir.display()))?;

        let mut file_contents = HashMap::new();

        for file in files {
            if file.exists() {
                let content = fs::read_to_string(file)
                    .with_context(|| format!("Failed to read {}", file.display()))?;

                file_contents.insert(file.clone(), content.clone());

                // Copy to backup directory
                if let Some(filename) = file.file_name() {
                    let backup_file = backup_dir.join(filename);
                    fs::write(&backup_file, &content)
                        .with_context(|| format!("Failed to write backup: {}", backup_file.display()))?;
                }
            }
        }

        println!(
            "{} Created backup at: {}",
            "✓".to_string(),
            backup_dir.display()
        );

        Ok(Self {
            timestamp,
            backup_dir,
            files: file_contents,
        })
    }

    /// Restore files from backup
    pub fn restore(&self) -> Result<()> {
        for (path, content) in &self.files {
            fs::write(path, content)
                .with_context(|| format!("Failed to restore {}", path.display()))?;
        }

        println!("{} Restored {} file(s) from backup", "✓", self.files.len());

        Ok(())
    }

    /// Delete the backup
    pub fn delete(&self) -> Result<()> {
        if self.backup_dir.exists() {
            fs::remove_dir_all(&self.backup_dir)
                .with_context(|| format!("Failed to delete backup: {}", self.backup_dir.display()))?;
        }
        Ok(())
    }

    /// Get the size of the backup
    pub fn size_bytes(&self) -> usize {
        self.files.values().map(|c| c.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_create_backup() {
        let temp = TempDir::new().unwrap();
        let test_file = temp.path().join("test.yml");

        let mut f = fs::File::create(&test_file).unwrap();
        writeln!(f, "test: content").unwrap();

        let backup = Backup::create(&[test_file.clone()], temp.path()).unwrap();

        assert_eq!(backup.files.len(), 1);
        assert!(backup.backup_dir.exists());
    }

    #[test]
    fn test_restore_backup() {
        let temp = TempDir::new().unwrap();
        let test_file = temp.path().join("test.yml");

        fs::write(&test_file, "original").unwrap();

        let backup = Backup::create(&[test_file.clone()], temp.path()).unwrap();

        // Modify file
        fs::write(&test_file, "modified").unwrap();

        // Restore
        backup.restore().unwrap();

        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "original");
    }
}
