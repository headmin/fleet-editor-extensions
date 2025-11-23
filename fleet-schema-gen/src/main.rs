mod sources;
mod schema;
mod generators;
mod utils;
mod linter;

use clap::{Parser, Subcommand};
use anyhow::Result;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "fleet-schema-gen")]
#[command(about = "Generate Fleet GitOps schemas for multiple editors", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate schemas from all sources
    Generate {
        /// Fleet version to generate schemas for
        #[arg(short, long)]
        fleet_version: Option<String>,

        /// Output directory
        #[arg(short, long, default_value = "./output")]
        output: PathBuf,

        /// Specific editor format (vscode, sublime, intellij, neovim, strict, all)
        #[arg(short, long, default_value = "all")]
        editor: String,

        /// Schema definitions directory
        #[arg(short, long, default_value = "./schema-defs")]
        schema_defs: PathBuf,

        /// Schema source: go (parse Fleet Go code), examples (infer from YAML), hybrid (both), or docs (scrape docs)
        #[arg(long, default_value = "hybrid")]
        source: String,
    },

    /// Update schemas from specific source
    Update {
        /// Source to update from (docs, github, openapi, local)
        #[arg(short, long)]
        source: String,

        /// Output directory
        #[arg(short, long, default_value = "./output")]
        output: PathBuf,
    },

    /// Lint YAML file(s) with Fleet-specific validation
    Lint {
        /// File or directory to lint
        path: PathBuf,

        /// Watch for changes and re-lint
        #[arg(short, long)]
        watch: bool,

        /// Automatically fix issues where possible
        #[arg(short, long)]
        fix: bool,

        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Validate YAML file against generated schema
    Validate {
        /// YAML file to validate
        file: PathBuf,

        /// Schema file to validate against
        #[arg(short, long)]
        schema: Option<PathBuf>,
    },

    /// Migrate Fleet config between versions
    Migrate {
        /// Path to config directory or file
        path: PathBuf,

        /// Source Fleet version (auto-detected if not specified)
        #[arg(short, long)]
        from: Option<String>,

        /// Target Fleet version (defaults to latest)
        #[arg(short, long)]
        to: Option<String>,

        /// Dry run - show changes without applying them
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Custom migrations file (default: migrations.toml)
        #[arg(short, long)]
        migrations_file: Option<PathBuf>,

        /// Create Git branch and commit
        #[arg(short, long)]
        git: bool,
    },

    /// Show diff between two Fleet versions
    Diff {
        /// Path to config directory
        path: PathBuf,

        /// First version
        #[arg(short, long)]
        from: String,

        /// Second version
        #[arg(short, long)]
        to: String,

        /// Show side-by-side diff
        #[arg(short, long)]
        side_by_side: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Generate { fleet_version, output, editor, schema_defs, source } => {
            println!("Generating schemas for Fleet version: {}",
                fleet_version.as_deref().unwrap_or("latest"));
            println!("Output directory: {}", output.display());
            println!("Editor format: {}", editor);
            println!("Schema source: {}", source);

            // Load and merge schema sources
            let schema = schema::build_schema(fleet_version, &schema_defs, &source).await?;

            // Generate based on editor choice
            match editor.as_str() {
                "vscode" => generators::vscode::generate(&schema, &output)?,
                "sublime" => generators::sublime::generate(&schema, &output)?,
                "intellij" => generators::intellij::generate(&schema, &output)?,
                "neovim" => generators::neovim::generate(&schema, &output)?,
                "strict" => generators::strict::generate(&schema, &output)?,
                "all" => {
                    generators::vscode::generate(&schema, &output.join("vscode"))?;
                    generators::sublime::generate(&schema, &output.join("sublime"))?;
                    generators::intellij::generate(&schema, &output.join("intellij"))?;
                    generators::neovim::generate(&schema, &output.join("neovim"))?;
                    generators::strict::generate(&schema, &output.join("strict"))?;
                }
                _ => anyhow::bail!("Unknown editor format: {}", editor),
            }

            println!("✓ Schema generation complete!");
        }

        Commands::Update { source, output } => {
            println!("Updating schemas from source: {}", source);

            match source.as_str() {
                "docs" => sources::docs_scraper::fetch_and_save(&output).await?,
                "github" => sources::github::fetch_and_save(&output).await?,
                "local" => println!("Local schemas already up to date"),
                _ => anyhow::bail!("Unknown source: {}", source),
            }

            println!("✓ Update complete!");
        }

        Commands::Lint { path, watch, fix, format } => {
            use linter::Linter;
            use colored::Colorize;

            if watch {
                println!("{} Watching {} for changes...", "👀".cyan(), path.display());
                // TODO: Implement watch mode
                anyhow::bail!("Watch mode not yet implemented");
            }

            if fix {
                println!("{} Auto-fix mode not yet implemented", "⚠️ ".yellow());
            }

            let linter = Linter::new();

            if path.is_file() {
                // Lint single file
                println!("{} Linting {}...\n", "🔍".blue(), path.display());

                let source = std::fs::read_to_string(&path)?;
                let report = linter.lint_file(&path)?;

                if format == "json" {
                    // TODO: JSON output
                    println!("JSON output not yet implemented");
                } else {
                    report.print(Some(&source));
                }

                if report.has_errors() {
                    std::process::exit(1);
                }
            } else if path.is_dir() {
                // Lint directory
                println!("{} Linting directory {}...\n", "🔍".blue(), path.display());

                let results = linter.lint_directory(&path, None)?;

                let mut total_errors = 0;
                let mut total_warnings = 0;
                let mut total_infos = 0;

                for (file_path, report) in &results {
                    if report.total_issues() > 0 {
                        println!("\n{} {}", "File:".bold(), file_path);

                        if let Ok(source) = std::fs::read_to_string(file_path) {
                            report.print(Some(&source));
                        } else {
                            report.print(None);
                        }

                        total_errors += report.errors.len();
                        total_warnings += report.warnings.len();
                        total_infos += report.infos.len();
                    }
                }

                // Overall summary
                println!("\n{}", "=".repeat(60));
                println!("{} Linted {} file(s)", "Summary:".bold(), results.len());
                println!("  {} error(s)", total_errors.to_string().red());
                println!("  {} warning(s)", total_warnings.to_string().yellow());
                println!("  {} info", total_infos.to_string().blue());

                if total_errors > 0 {
                    std::process::exit(1);
                }
            } else {
                anyhow::bail!("Path does not exist: {}", path.display());
            }
        }

        Commands::Validate { file, schema } => {
            use linter::Linter;

            println!("🔍 Validating: {}", file.display());

            let linter = Linter::new();
            let source = std::fs::read_to_string(&file)?;
            let report = linter.lint_file(&file)?;

            report.print(Some(&source));

            if report.has_errors() {
                std::process::exit(1);
            }

            println!("✓ Validation complete!");
        }

        Commands::Migrate {
            path,
            from,
            to,
            dry_run,
            migrations_file,
            git,
        } => {
            use linter::migrate::{Migrator, Version};
            use colored::Colorize;

            let mut migrator = Migrator::new();

            // Load custom migrations if specified
            if let Some(migrations_path) = migrations_file {
                println!("{} Loading migrations from: {}",
                    "→".blue().bold(),
                    migrations_path.display()
                );
                migrator.load_migrations_from_file(&migrations_path)?;
            }

            // Auto-detect source version if not specified
            let from_version = if let Some(v) = from {
                Version::parse(&v)
                    .ok_or_else(|| anyhow::anyhow!("Invalid version format: {}", v))?
            } else {
                println!("{} Auto-detecting Fleet version...", "→".blue().bold());
                migrator.detect_version(&path)?
                    .ok_or_else(|| anyhow::anyhow!("Could not auto-detect Fleet version. Please specify with --from"))?
            };

            // Use latest version if target not specified
            let to_version = if let Some(v) = to {
                Version::parse(&v)
                    .ok_or_else(|| anyhow::anyhow!("Invalid version format: {}", v))?
            } else {
                migrator.latest_version()
            };

            println!("\n{} Migrating Fleet config: {} → {}",
                "🔄".cyan(),
                from_version.to_string().yellow(),
                to_version.to_string().green()
            );
            println!("{} Path: {}\n",
                "→".blue().bold(),
                path.display()
            );

            // Create migration plan
            let plan = migrator.plan_migration(&path, &from_version, &to_version)?;

            println!("{} Migration plan created:",
                "✓".green()
            );
            println!("  • {} migration(s) to apply",
                plan.migrations.len().to_string().bold()
            );
            println!("  • {} file(s) affected",
                plan.affected_files.len().to_string().bold()
            );
            println!("  • {} estimated change(s)\n",
                plan.estimated_changes.to_string().bold()
            );

            // Execute migration
            if git && !dry_run {
                use linter::migrate::git::GitMigrator;
                use std::path::Path;

                println!("{} Creating Git branch...", "→".blue().bold());
                let git_migrator = GitMigrator::open(Path::new("."))?;
                let branch_name = git_migrator.create_migration_branch(
                    &from_version.to_string(),
                    &to_version.to_string()
                )?;
                println!("{} Created branch: {}\n",
                    "✓".green(),
                    branch_name.bold()
                );
            }

            migrator.execute_migration(&plan, dry_run)?;

            if git && !dry_run {
                use linter::migrate::git::GitMigrator;
                use std::path::Path;

                println!("\n{} Creating commit...", "→".blue().bold());
                let git_migrator = GitMigrator::open(Path::new("."))?;
                git_migrator.commit_migration(
                    &from_version.to_string(),
                    &to_version.to_string(),
                    plan.affected_files.len()
                )?;
                println!("{} Migration committed", "✓".green());
            }
        }

        Commands::Diff {
            path,
            from,
            to,
            side_by_side,
        } => {
            use linter::migrate::{Migrator, Version};
            use colored::Colorize;

            println!("\n{} Analyzing migration changes: {} → {}",
                "📊".cyan(),
                from.yellow(),
                to.green()
            );

            let mut migrator = Migrator::new();
            let from_version = Version::parse(&from)
                .ok_or_else(|| anyhow::anyhow!("Invalid version format: {}", from))?;
            let to_version = Version::parse(&to)
                .ok_or_else(|| anyhow::anyhow!("Invalid version format: {}", to))?;

            // Create migration plan
            let plan = migrator.plan_migration(&path, &from_version, &to_version)?;

            println!("\n{} Migration overview:\n",
                "→".blue().bold()
            );

            // Show each migration
            for migration in &plan.migrations {
                println!("{} {}",
                    "Migration:".bold(),
                    migration.id.cyan()
                );
                println!("  {} → {}",
                    migration.from_version.to_string().dimmed(),
                    migration.to_version.to_string().dimmed()
                );
                println!("  {}",
                    migration.description.italic()
                );
                println!();

                // Show transformations
                for transformation in &migration.transformations {
                    match transformation {
                        linter::migrate::types::Transformation::FieldMove {
                            source_pattern,
                            target_pattern,
                            fields,
                            target_location,
                            ..
                        } => {
                            println!("  {} Field Move",
                                "•".blue()
                            );
                            println!("    {} {}",
                                "From:".dimmed(),
                                source_pattern
                            );
                            println!("    {} {}",
                                "To:".dimmed(),
                                target_pattern
                            );
                            println!("    {} {}",
                                "Location:".dimmed(),
                                target_location
                            );
                            println!("    {} {}",
                                "Fields:".dimmed(),
                                fields.join(", ").yellow()
                            );
                        }
                        linter::migrate::types::Transformation::FieldRename {
                            pattern,
                            old_path,
                            new_path,
                        } => {
                            println!("  {} Field Rename in {}",
                                "•".blue(),
                                pattern
                            );
                            println!("    {} → {}",
                                old_path.red(),
                                new_path.green()
                            );
                        }
                        linter::migrate::types::Transformation::FieldDelete {
                            pattern,
                            fields,
                            reason,
                        } => {
                            println!("  {} Field Delete in {}",
                                "•".blue(),
                                pattern
                            );
                            println!("    {}",
                                fields.join(", ").red()
                            );
                            if let Some(r) = reason {
                                println!("    {} {}",
                                    "Reason:".dimmed(),
                                    r.italic()
                                );
                            }
                        }
                        linter::migrate::types::Transformation::Restructure {
                            name,
                            description,
                        } => {
                            println!("  {} Restructure: {}",
                                "•".blue(),
                                name.bold()
                            );
                            println!("    {}",
                                description.italic()
                            );
                        }
                    }
                    println!();
                }
            }

            println!("{}", "─".repeat(60).dimmed());
            println!("\n{} Summary:",
                "Summary:".bold()
            );
            println!("  {} migration(s)",
                plan.migrations.len().to_string().cyan()
            );
            println!("  {} file(s) would be affected",
                plan.affected_files.len().to_string().yellow()
            );
            println!("  {} estimated change(s)",
                plan.estimated_changes.to_string().yellow()
            );

            if !plan.affected_files.is_empty() {
                println!("\n{} Affected files:",
                    "Files:".bold()
                );
                for file in &plan.affected_files {
                    println!("  • {}",
                        file.display().to_string().dimmed()
                    );
                }
            }

            println!("\n{} Run with {} to see the actual changes",
                "Tip:".blue().bold(),
                "migrate --dry-run".yellow()
            );
        }
    }

    Ok(())
}
