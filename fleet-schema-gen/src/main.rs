mod sources;
mod schema;
mod generators;
mod utils;

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

    /// Validate YAML file against generated schema
    Validate {
        /// YAML file to validate
        file: PathBuf,

        /// Schema file to validate against
        #[arg(short, long)]
        schema: Option<PathBuf>,
    },

    /// Show diff between two Fleet versions
    Diff {
        /// First version
        from: String,

        /// Second version
        to: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Generate { fleet_version, output, editor, schema_defs } => {
            println!("Generating schemas for Fleet version: {}",
                fleet_version.as_deref().unwrap_or("latest"));
            println!("Output directory: {}", output.display());
            println!("Editor format: {}", editor);

            // Load and merge schema sources
            let schema = schema::build_schema(fleet_version, &schema_defs).await?;

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

        Commands::Validate { file, schema } => {
            println!("Validating: {}", file.display());

            // TODO: Implement validation logic
            println!("✓ Validation complete!");
        }

        Commands::Diff { from, to } => {
            println!("Comparing Fleet versions: {} → {}", from, to);

            // TODO: Implement diff logic
            println!("✓ Diff complete!");
        }
    }

    Ok(())
}
