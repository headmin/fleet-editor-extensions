//! `flint init` — scaffold a fleetlint.toml.
//!
//! The whole flow lives here: detect, report, ask, generate, write, report.
//! `flint_lint` supplies the pure pieces (`detect_workspace`, `scope::*`,
//! `generate_config_toml`, `write_config`) and prints nothing, so the
//! ordering of output and questions is decided in one place instead of being
//! split across a library boundary.

use crate::args::InitArgs;
use crate::interactive::scope::{ask_scope, ask_strictness};
use colored::Colorize;
use flint_lint::{self as linter, UserAnswers};

pub(crate) fn run(args: InitArgs) -> anyhow::Result<()> {
    let root = std::env::current_dir()?;
    let config_path = linter::config_path_for(&root, args.output);

    // Fail before doing any work, not after the user has answered questions.
    if config_path.exists() && !args.force {
        anyhow::bail!(
            "Configuration file already exists: {}\nUse --force to overwrite.",
            config_path.display()
        );
    }

    println!("{} Detecting Fleet GitOps structure...\n", "🔍".cyan());
    let detected = linter::detect_workspace(&root);
    print_detected(&detected);

    // The scope walk reads every YAML file to resolve `path:`/`paths:`
    // references, so it only runs when someone is there to answer for it.
    let answers = if args.no_interactive {
        UserAnswers::default()
    } else {
        let strictness = ask_strictness()?;
        let scan = linter::scope::scan(&root);
        let selection = ask_scope(&scan)?;
        UserAnswers {
            strictness,
            scope: linter::scope::preview(&scan, &selection),
        }
    };

    let content = linter::generate_config_toml(&detected, &answers);
    linter::write_config(&config_path, &content, args.force)?;

    println!(
        "\n{} Created {}",
        "✓".green().bold(),
        config_path.display().to_string().bold()
    );
    if !answers.scope.include.is_empty() {
        println!(
            "  {} {} of {} file(s) in scope",
            "•".dimmed(),
            answers.scope.in_scope.to_string().cyan(),
            answers.scope.total
        );
    }
    println!("\n{}:", "Next steps".bold());
    println!("  • Run {} to validate your configs", "flint check .".cyan());
    println!(
        "  • Edit {} to customize rules",
        config_path.display().to_string().cyan()
    );

    Ok(())
}

/// Render what the structure scan found, plus the legacy-rename nudges.
fn print_detected(d: &linter::DetectedConfig) {
    println!("{}:", "Found".bold());
    println!("  • {} YAML file(s)", d.yaml_file_count.to_string().cyan());

    if d.has_fleets_dir {
        let (dir, count) = if d.has_legacy_teams_dir {
            ("teams/".yellow(), d.fleet_count)
        } else {
            ("fleets/".green(), d.fleet_count)
        };
        println!("  • {dir} directory with {count} fleet(s)");
    }
    if d.has_lib_dir {
        println!(
            "  • {} directory {}",
            "lib/".yellow(),
            "(deprecated legacy layout — migrate to platforms/)".dimmed()
        );
    }
    if !d.root_yaml_files.is_empty() {
        println!("  • Root files: {}", d.root_yaml_files.join(", ").dimmed());
    }
    if !d.detected_platforms.is_empty() {
        println!("  • Platforms: {}", d.detected_platforms.join(", ").yellow());
    }
    if d.has_path_references {
        println!("  • Path references detected (cross-file includes)");
    }

    if d.has_legacy_teams_dir {
        println!(
            "\n{}",
            "⚠ Found 'teams/' directory — Fleet is renaming this to 'fleets/'.".yellow()
        );
        println!("  Consider renaming: {}", "mv teams/ fleets/".cyan());
    }
    if d.has_legacy_queries {
        println!(
            "\n{}",
            "⚠ Found 'queries:' key — Fleet is renaming this to 'reports:'.".yellow()
        );
        println!(
            "  Consider updating your YAML files to use {} instead.",
            "reports:".cyan()
        );
    }
}
