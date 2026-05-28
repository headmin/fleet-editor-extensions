//! Fleet GitOps YAML linter — lint engine, rules, schema, and configuration.
//!
//! This crate provides the core linting logic used by both the CLI (`flint check`)
//! and the LSP server (`flint lsp`). It is designed as a reusable library with no
//! I/O assumptions beyond file reading.

pub mod config;
pub mod deprecation_rule;
pub mod deprecations;
pub mod engine;
pub mod error;
pub mod fleet_config;
pub mod help_agents;
pub mod init;
pub mod osquery;
pub mod overlay;
pub mod rules;
pub mod self_reference;
pub mod semantic;
pub mod structural;
pub mod structure;
pub mod version;
pub mod version_gate;
pub mod yaml_lint;
pub mod yaml_utils;

pub use config::{ConfigError, FleetConnectionConfig, FleetLintConfig};
pub use deprecations::{Deprecation, DeprecationKind, DeprecationPhase, DEPRECATION_REGISTRY};
pub use engine::Linter;
pub use error::{FixSafety, LintError, LintResult, Severity};
pub use fleet_config::FleetConfig;
pub use init::init as init_config;
pub use overlay::{merge_yaml, OverlayError};
pub use rules::{Rule, RuleSet};
pub use version::Version;
pub use version_gate::VersionContext;
