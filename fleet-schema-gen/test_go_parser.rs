// Simple test for Go parser
use fleet_schema_gen::sources::go_parser::FleetGoParser;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    println!("Testing Go parser...\n");

    let mut parser = FleetGoParser::new()?;

    // Parse just the policies.go file
    let file_path = Path::new("/tmp/fleet/server/fleet/policies.go");
    println!("Parsing: {}", file_path.display());

    parser.parse_file(file_path)?;

    println!("\n✓ Successfully parsed!");
    println!("Found {} struct definitions\n", parser.struct_cache.len());

    // Show what we found
    for (name, go_struct) in parser.struct_cache.iter() {
        println!("Struct: {}", name);
        println!("  Fields: {}", go_struct.fields.len());
        for field in go_struct.fields.iter().take(3) {
            println!("    - {} : {} (json: {:?})",
                field.name,
                field.go_type,
                field.json_tag
            );
        }
        if go_struct.fields.len() > 3 {
            println!("    ... and {} more", go_struct.fields.len() - 3);
        }
        println!();
    }

    Ok(())
}
