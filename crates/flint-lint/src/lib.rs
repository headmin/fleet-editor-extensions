//! Fleet GitOps YAML linter — lint engine, rules, schema, and configuration.
//!
//! This crate provides the core linting logic used by both the CLI (`flint check`)
//! and the LSP server (`flint lsp`). It is designed as a reusable library with no
//! I/O assumptions beyond file reading.
//!
//! The public surface is deliberately curated: the diagnostic model
//! ([`error`], [`fix`], [`codes`]), the engine facade ([`Linter`] and the
//! root re-exports below), the data registries the LSP renders from
//! ([`osquery`], [`deprecations`], [`fleet_config`]), the repo-wide index
//! ([`cross_reference`]), the `init` scope analysis the CLI prompts over
//! ([`scope`]), and the artifact-generator building blocks
//! ([`pkg`], [`installers`], [`profile`], [`query_gen`], [`unwired`]).
//! Individual rule modules and the schema tables are implementation details.

// -- public surface -----------------------------------------------------
pub mod codes;
pub mod cross_reference;
pub mod deprecations;
pub mod error;
pub mod fix;
pub mod fleet_config;
pub mod fma;
pub mod installers;
pub mod osquery;
pub mod pkg;
pub mod profile;
pub mod query_gen;
pub mod rules;
pub mod scope;
pub mod snapshot;
pub mod unwired;
pub mod workspace;

// -- implementation details (reachable only via the re-exports below) ---
mod config;
mod deprecation_rule;
mod engine;
mod init;
mod overlay;
mod path_exists;
mod patterns;
mod self_reference;
mod semantic;
mod structural;
mod structure;
mod util;
mod version;
mod version_gate;
mod yaml_lint;
mod yaml_utils;

pub use config::{ConfigError, FleetConnectionConfig, FleetLintConfig};
pub use deprecations::{Deprecation, DeprecationKind, DeprecationPhase, DEPRECATION_REGISTRY};
pub use engine::Linter;
pub use error::{Fix, FixSafety, LintError, LintReport, LintResult, Severity, Span};
pub use fix::ApplyMode;
pub use fleet_config::FleetConfig;
pub use init::init as init_config;
pub use init::{discover_gitops_root, parse_strictness, DetectedConfig, InitPrompts, StrictnessLevel};
pub use overlay::{merge_yaml, OverlayError};
pub use rules::{Rule, RuleOptions, RuleSet};
pub use version::Version;
pub use version_gate::VersionContext;
