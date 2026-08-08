//! `flint query` — a Fleet query/policy stanza from a .sql file.

use crate::args::QueryArgs;
use flint_lint as linter;

pub(crate) fn run(args: QueryArgs) -> anyhow::Result<()> {
    let QueryArgs {
        path,
        policy,
        output,
    } = args;

    use colored::Colorize;
    let stanza = build_query_stanza(&path, policy)?;
    if let Some(out) = output {
        use std::io::Write;
        let existed = out.exists();
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&out)?;
        writeln!(f, "{stanza}")?;
        println!(
            "{} {} stanza to {}",
            "✓".green(),
            if existed { "appended" } else { "wrote" },
            out.display()
        );
    } else {
        println!("{stanza}");
    }
    Ok(())
}

/// Build a Fleet query (or policy) stanza from a `.sql` file, inferring
/// `platform:` from the osquery tables it references.
fn build_query_stanza(path: &std::path::Path, policy: bool) -> anyhow::Result<String> {
    use linter::query_gen::infer_platforms;

    if !path.exists() {
        anyhow::bail!("file not found: {}", path.display());
    }
    let sql = std::fs::read_to_string(path)?;
    let body = sql.trim();
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("New Query")
        .replace(['_', '-'], " ");

    let platform_line = match infer_platforms(&sql) {
        p if p.is_empty() => {
            "  # platform: darwin   # could not infer — set target platform(s)".to_string()
        }
        p => format!("  platform: {}", p.join(",")),
    };
    let query_block: String = body
        .lines()
        .map(|l| format!("    {l}"))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(if policy {
        format!(
            "- name: {name}\n  query: |\n{query_block}\n{platform_line}\n  # resolution: \"How to remediate\"\n  # critical: false"
        )
    } else {
        format!(
            "- name: {name}\n  query: |\n{query_block}\n{platform_line}\n  interval: 3600\n  # observer_can_run: true"
        )
    })
}
