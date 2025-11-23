use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fmt;

/// Fleet version
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self { major, minor, patch }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() < 2 {
            return None;
        }

        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        let patch = parts.get(2).and_then(|p| p.parse().ok()).unwrap_or(0);

        Some(Self::new(major, minor, patch))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// A complete migration from one version to another
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    pub id: String,
    pub from_version: Version,
    pub to_version: Version,
    pub description: String,
    pub transformations: Vec<Transformation>,
}

/// Types of transformations that can be applied
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Transformation {
    /// Move fields from one file to another
    FieldMove {
        source_pattern: String,
        target_pattern: String,
        fields: Vec<String>,
        match_strategy: MatchStrategy,
        target_location: String,
    },
    /// Rename a field (can handle nested paths)
    FieldRename {
        pattern: String,
        old_path: String,
        new_path: String,
    },
    /// Delete a field (for deprecations)
    FieldDelete {
        pattern: String,
        fields: Vec<String>,
        reason: Option<String>,
    },
    /// Complex restructuring (custom logic)
    Restructure {
        name: String,
        description: String,
    },
}

/// How to match source and target files for FieldMove
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MatchStrategy {
    /// Match via path references (e.g., `path: ../lib/software/chrome.yml`)
    PathReference,
    /// Match by file name
    FileName,
    /// Match by custom logic
    Custom(String),
}

/// Plan for executing a migration
#[derive(Debug, Clone)]
pub struct MigrationPlan {
    pub migrations: Vec<Migration>,
    pub affected_files: Vec<PathBuf>,
    pub steps: Vec<MigrationStep>,
    pub estimated_changes: usize,
}

/// A single step in a migration plan
#[derive(Debug, Clone)]
pub struct MigrationStep {
    pub description: String,
    pub file: PathBuf,
    pub changes: Vec<FileChange>,
}

/// A change to be made to a file
#[derive(Debug, Clone)]
pub enum FileChange {
    AddField {
        path: String,
        value: serde_yaml::Value,
    },
    RemoveField {
        path: String,
    },
    RenameField {
        old_path: String,
        new_path: String,
    },
    ModifyValue {
        path: String,
        old_value: serde_yaml::Value,
        new_value: serde_yaml::Value,
    },
}

/// Result of version detection
#[derive(Debug, Clone)]
pub struct DetectionResult {
    pub version: Option<Version>,
    pub confidence: f32, // 0.0 to 1.0
    pub indicators: Vec<String>, // What led to this detection
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing() {
        assert_eq!(
            Version::parse("4.74.0"),
            Some(Version::new(4, 74, 0))
        );
        assert_eq!(
            Version::parse("4.73"),
            Some(Version::new(4, 73, 0))
        );
        assert_eq!(Version::parse("invalid"), None);
    }

    #[test]
    fn test_version_comparison() {
        let v473 = Version::new(4, 73, 0);
        let v474 = Version::new(4, 74, 0);

        assert!(v473 < v474);
        assert!(v474 > v473);
        assert_eq!(v473, v473.clone());
    }

    #[test]
    fn test_version_display() {
        let v = Version::new(4, 74, 0);
        assert_eq!(format!("{}", v), "4.74.0");
    }
}
