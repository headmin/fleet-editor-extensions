// Comprehensive test for Go parser - parse all Fleet schema files

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use fleet_schema_gen::sources::go_parser::FleetGoParser;
    use fleet_schema_gen::sources::fleet_repo::FleetRepo;

    println!("Testing Go parser with complete Fleet repository...\n");

    // Ensure Fleet repo is available
    let fleet_repo = FleetRepo::new();
    fleet_repo.ensure_repo(None)?;

    println!("Repository path: {}\n", fleet_repo.path().display());

    let mut parser = FleetGoParser::new()?;

    // Parse all key schema files
    let files_to_parse = vec![
        ("pkg/spec/gitops.go", "GitOps definitions"),
        ("server/fleet/teams.go", "Team configuration"),
        ("server/fleet/policies.go", "Policy specs"),
        ("server/fleet/queries.go", "Query specs"),
        ("server/fleet/labels.go", "Label specs"),
        ("server/fleet/software.go", "Software specs"),
        ("server/fleet/mdm.go", "MDM controls"),
    ];

    let mut total_structs = 0;
    let mut total_fields = 0;

    for (file_path, description) in files_to_parse {
        let full_path = fleet_repo.path().join(file_path);

        if !full_path.exists() {
            eprintln!("  ⚠ File not found: {}", file_path);
            continue;
        }

        println!("📄 Parsing: {} ({})", file_path, description);

        let before_count = parser.struct_cache.len();

        match parser.parse_file(&full_path) {
            Ok(_) => {
                let new_structs = parser.struct_cache.len() - before_count;
                println!("  ✓ Found {} new struct(s)", new_structs);
                total_structs += new_structs;
            }
            Err(e) => {
                eprintln!("  ✗ Error: {}", e);
            }
        }

        println!();
    }

    println!("{}", "=".repeat(60));
    println!("Total structs parsed: {}", parser.struct_cache.len());
    println!("{}", "=".repeat(60));
    println!();

    // Show interesting structs
    let interesting = vec!["GitOps", "PolicySpec", "QuerySpec", "LabelSpec", "Policy", "Query", "Label"];

    for name in &interesting {
        if let Some(go_struct) = parser.struct_cache.get(&name.to_string()) {
            println!("📦 Struct: {}", name);
            println!("  Fields: {}", go_struct.fields.len());

            for field in go_struct.fields.iter().take(5) {
                println!("    - {} : {} (json: {:?}, omitempty: {})",
                    field.name,
                    field.go_type,
                    field.json_tag,
                    field.omitempty
                );
                total_fields += 1;
            }

            if go_struct.fields.len() > 5 {
                println!("    ... and {} more fields", go_struct.fields.len() - 5);
                total_fields += go_struct.fields.len() - 5;
            }

            println!();
        }
    }

    println!("{}", "=".repeat(60));
    println!("Summary:");
    println!("  Total structs: {}", parser.struct_cache.len());
    println!("  Interesting structs shown: {}", interesting.iter().filter(|n| parser.struct_cache.contains_key(&n.to_string())).count());
    println!("{}", "=".repeat(60));

    Ok(())
}
