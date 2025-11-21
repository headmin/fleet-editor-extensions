use anyhow::Result;
use std::path::Path;
use std::fs;
use serde_json::json;
use crate::schema::types::FleetSchema;

pub fn generate(schema: &FleetSchema, output_dir: &Path) -> Result<()> {
    println!("\n=== Generating IntelliJ IDEA Configuration ===");

    fs::create_dir_all(output_dir)?;

    // 1. Generate JSON schemas for IntelliJ
    generate_schemas(schema, output_dir)?;

    // 2. Generate schema mappings
    generate_schema_mappings(output_dir)?;

    // 3. Generate live templates (snippets)
    generate_live_templates(output_dir)?;

    // 4. Generate file templates
    generate_file_templates(output_dir)?;

    // 5. Generate project settings
    generate_project_settings(output_dir)?;

    // 6. Generate README
    generate_readme(output_dir)?;

    println!("✓ IntelliJ IDEA configuration generated at: {}", output_dir.display());

    Ok(())
}

fn generate_schemas(schema: &FleetSchema, output_dir: &Path) -> Result<()> {
    println!("\n  → Generating JSON schemas for IntelliJ...");

    let schemas_dir = output_dir.join("schemas");
    fs::create_dir_all(&schemas_dir)?;

    // IntelliJ uses standard JSON Schema (Draft-07 or 2019-09)
    let schemas = vec![
        ("fleet-default.schema.json", &schema.default_schema, "Fleet Default Configuration"),
        ("fleet-team.schema.json", &schema.team_schema, "Fleet Team Configuration"),
        ("fleet-policy.schema.json", &schema.policy_schema, "Fleet Policy"),
        ("fleet-query.schema.json", &schema.query_schema, "Fleet Query"),
        ("fleet-label.schema.json", &schema.label_schema, "Fleet Label"),
    ];

    for (filename, schema_def, title) in schemas {
        let mut output_schema = schema_def.clone();
        output_schema.title = Some(title.to_string());

        // IntelliJ prefers Draft-07
        output_schema.schema = Some("http://json-schema.org/draft-07/schema#".to_string());

        let json = serde_json::to_string_pretty(&output_schema)?;
        fs::write(schemas_dir.join(filename), json)?;

        println!("    ✓ {}", filename);
    }

    Ok(())
}

fn generate_schema_mappings(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating schema mappings...");

    // IntelliJ schema mappings are configured in .idea/jsonSchemas.xml
    let mappings = r#"<?xml version="1.0" encoding="UTF-8"?>
<project version="4">
  <component name="JsonSchemaMappingsProjectConfiguration">
    <state>
      <map>
        <entry key="Fleet Default Configuration">
          <value>
            <SchemaInfo>
              <option name="name" value="Fleet Default Configuration" />
              <option name="relativePathToSchema" value="intellij/schemas/fleet-default.schema.json" />
              <option name="patterns">
                <list>
                  <Item>
                    <option name="path" value="default.yml" />
                  </Item>
                  <Item>
                    <option name="path" value="default.yaml" />
                  </Item>
                </list>
              </option>
            </SchemaInfo>
          </value>
        </entry>
        <entry key="Fleet Team Configuration">
          <value>
            <SchemaInfo>
              <option name="name" value="Fleet Team Configuration" />
              <option name="relativePathToSchema" value="intellij/schemas/fleet-team.schema.json" />
              <option name="patterns">
                <list>
                  <Item>
                    <option name="path" value="teams/*.yml" />
                  </Item>
                  <Item>
                    <option name="path" value="teams/*.yaml" />
                  </Item>
                </list>
              </option>
            </SchemaInfo>
          </value>
        </entry>
        <entry key="Fleet Policy">
          <value>
            <SchemaInfo>
              <option name="name" value="Fleet Policy" />
              <option name="relativePathToSchema" value="intellij/schemas/fleet-policy.schema.json" />
              <option name="patterns">
                <list>
                  <Item>
                    <option name="path" value="lib/policies/*.yml" />
                  </Item>
                  <Item>
                    <option name="path" value="lib/policies/*.yaml" />
                  </Item>
                </list>
              </option>
            </SchemaInfo>
          </value>
        </entry>
        <entry key="Fleet Query">
          <value>
            <SchemaInfo>
              <option name="name" value="Fleet Query" />
              <option name="relativePathToSchema" value="intellij/schemas/fleet-query.schema.json" />
              <option name="patterns">
                <list>
                  <Item>
                    <option name="path" value="lib/queries/*.yml" />
                  </Item>
                  <Item>
                    <option name="path" value="lib/queries/*.yaml" />
                  </Item>
                </list>
              </option>
            </SchemaInfo>
          </value>
        </entry>
        <entry key="Fleet Label">
          <value>
            <SchemaInfo>
              <option name="name" value="Fleet Label" />
              <option name="relativePathToSchema" value="intellij/schemas/fleet-label.schema.json" />
              <option name="patterns">
                <list>
                  <Item>
                    <option name="path" value="lib/labels/*.yml" />
                  </Item>
                  <Item>
                    <option name="path" value="lib/labels/*.yaml" />
                  </Item>
                </list>
              </option>
            </SchemaInfo>
          </value>
        </entry>
      </map>
    </state>
  </component>
</project>
"#;

    let idea_dir = output_dir.join(".idea");
    fs::create_dir_all(&idea_dir)?;
    fs::write(idea_dir.join("jsonSchemas.xml"), mappings)?;

    println!("    ✓ jsonSchemas.xml");

    Ok(())
}

fn generate_live_templates(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating live templates (snippets)...");

    // IntelliJ Live Templates are stored in XML format
    let templates = r#"<templateSet group="Fleet GitOps">
  <template name="fleet-policy" value="- name: &quot;$PLATFORM$ - $NAME$&quot;&#10;  description: &quot;$DESCRIPTION$&quot;&#10;  query: &quot;$QUERY$&quot;&#10;  platform: &quot;$PLATFORM_ENUM$&quot;&#10;  critical: $CRITICAL$" description="Create a Fleet policy" toReformat="true" toShortenFQNames="true">
    <variable name="PLATFORM" expression="" defaultValue="&quot;macOS&quot;" alwaysStopAt="true" />
    <variable name="NAME" expression="" defaultValue="&quot;Firewall enabled&quot;" alwaysStopAt="true" />
    <variable name="DESCRIPTION" expression="" defaultValue="&quot;Ensure the system firewall is enabled&quot;" alwaysStopAt="true" />
    <variable name="QUERY" expression="" defaultValue="&quot;SELECT 1 FROM alf WHERE global_state &gt;= 1;&quot;" alwaysStopAt="true" />
    <variable name="PLATFORM_ENUM" expression="enum(&quot;darwin&quot;, &quot;windows&quot;, &quot;linux&quot;, &quot;chrome&quot;)" defaultValue="&quot;darwin&quot;" alwaysStopAt="true" />
    <variable name="CRITICAL" expression="enum(&quot;false&quot;, &quot;true&quot;)" defaultValue="&quot;false&quot;" alwaysStopAt="true" />
    <context>
      <option name="YAML" value="true" />
    </context>
  </template>

  <template name="fleet-query" value="- name: &quot;$NAME$&quot;&#10;  query: &quot;$QUERY$&quot;&#10;  description: &quot;$DESCRIPTION$&quot;&#10;  interval: $INTERVAL$&#10;  platform: &quot;$PLATFORM$&quot;" description="Create a Fleet query" toReformat="true" toShortenFQNames="true">
    <variable name="NAME" expression="" defaultValue="&quot;get_usb_devices&quot;" alwaysStopAt="true" />
    <variable name="QUERY" expression="" defaultValue="&quot;SELECT * FROM usb_devices;&quot;" alwaysStopAt="true" />
    <variable name="DESCRIPTION" expression="" defaultValue="&quot;List all connected USB devices&quot;" alwaysStopAt="true" />
    <variable name="INTERVAL" expression="" defaultValue="3600" alwaysStopAt="true" />
    <variable name="PLATFORM" expression="enum(&quot;darwin&quot;, &quot;windows&quot;, &quot;linux&quot;, &quot;chrome&quot;)" defaultValue="&quot;darwin&quot;" alwaysStopAt="true" />
    <context>
      <option name="YAML" value="true" />
    </context>
  </template>

  <template name="fleet-label" value="- name: &quot;$NAME$&quot;&#10;  query: &quot;$QUERY$&quot;&#10;  description: &quot;$DESCRIPTION$&quot;" description="Create a Fleet label" toReformat="true" toShortenFQNames="true">
    <variable name="NAME" expression="" defaultValue="&quot;macOS laptops&quot;" alwaysStopAt="true" />
    <variable name="QUERY" expression="" defaultValue="&quot;SELECT 1 FROM system_info WHERE hardware_model LIKE '%Book%';&quot;" alwaysStopAt="true" />
    <variable name="DESCRIPTION" expression="" defaultValue="&quot;All macOS laptop devices&quot;" alwaysStopAt="true" />
    <context>
      <option name="YAML" value="true" />
    </context>
  </template>

  <template name="fleet-control-macos" value="macos_settings:&#10;  custom_settings:&#10;    - path: &quot;$PATH$&quot;&#10;      labels:&#10;        - &quot;$LABEL$&quot;" description="Create macOS custom settings control" toReformat="true" toShortenFQNames="true">
    <variable name="PATH" expression="" defaultValue="&quot;./lib/macos/profiles/security.mobileconfig&quot;" alwaysStopAt="true" />
    <variable name="LABEL" expression="" defaultValue="&quot;macOS laptops&quot;" alwaysStopAt="true" />
    <context>
      <option name="YAML" value="true" />
    </context>
  </template>

  <template name="fleet-software-package" value="- software:&#10;    - url: &quot;$URL$&quot;&#10;      install_script: &quot;$INSTALL_SCRIPT$&quot;&#10;      uninstall_script: &quot;$UNINSTALL_SCRIPT$&quot;&#10;      pre_install_query: &quot;$PRE_INSTALL_QUERY$&quot;" description="Create a software package definition" toReformat="true" toShortenFQNames="true">
    <variable name="URL" expression="" defaultValue="&quot;https://example.com/package.pkg&quot;" alwaysStopAt="true" />
    <variable name="INSTALL_SCRIPT" expression="" defaultValue="&quot;./lib/scripts/install.sh&quot;" alwaysStopAt="true" />
    <variable name="UNINSTALL_SCRIPT" expression="" defaultValue="&quot;./lib/scripts/uninstall.sh&quot;" alwaysStopAt="true" />
    <variable name="PRE_INSTALL_QUERY" expression="" defaultValue="&quot;SELECT 1;&quot;" alwaysStopAt="true" />
    <context>
      <option name="YAML" value="true" />
    </context>
  </template>
</templateSet>
"#;

    let templates_dir = output_dir.join("templates");
    fs::create_dir_all(&templates_dir)?;
    fs::write(templates_dir.join("Fleet-GitOps.xml"), templates)?;

    println!("    ✓ Fleet-GitOps.xml (live templates)");

    Ok(())
}

fn generate_file_templates(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating file templates...");

    let file_templates_dir = output_dir.join("fileTemplates");
    fs::create_dir_all(&file_templates_dir)?;

    // Policy file template
    let policy_template = r#"---
apiVersion: v1
kind: policy
spec:
  name: "${NAME}"
  description: "${DESCRIPTION}"
  query: "${QUERY}"
  platform: "${PLATFORM}"
  critical: false
"#;

    fs::write(file_templates_dir.join("Fleet Policy.yml"), policy_template)?;
    println!("    ✓ Fleet Policy.yml");

    // Query file template
    let query_template = r#"---
apiVersion: v1
kind: query
spec:
  name: "${NAME}"
  query: "${QUERY}"
  description: "${DESCRIPTION}"
  interval: 3600
"#;

    fs::write(file_templates_dir.join("Fleet Query.yml"), query_template)?;
    println!("    ✓ Fleet Query.yml");

    // Team file template
    let team_template = r#"---
apiVersion: v1
kind: team
spec:
  team:
    name: "${TEAM_NAME}"

  # Policies for this team
  policies:
    - name: "${TEAM_NAME} - Example policy"
      description: "Example policy for this team"
      query: "SELECT 1;"
      platform: "darwin"
      critical: false

  # Team settings
  team_settings:
    secrets:
      - name: "${TEAM_NAME}_SECRET"
        secret: ""
"#;

    fs::write(file_templates_dir.join("Fleet Team.yml"), team_template)?;
    println!("    ✓ Fleet Team.yml");

    Ok(())
}

fn generate_project_settings(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating project settings...");

    let idea_dir = output_dir.join(".idea");
    fs::create_dir_all(&idea_dir)?;

    // Generate misc.xml for project settings
    let misc_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<project version="4">
  <component name="ProjectRootManager">
    <output url="file://$PROJECT_DIR$/out" />
  </component>
  <component name="ProjectType">
    <option name="id" value="fleet-gitops" />
  </component>
</project>
"#;

    fs::write(idea_dir.join("misc.xml"), misc_xml)?;
    println!("    ✓ misc.xml");

    // Generate modules.xml
    let modules_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<project version="4">
  <component name="ProjectModuleManager">
    <modules>
      <module fileurl="file://$PROJECT_DIR$/.idea/fleet-gitops.iml" filepath="$PROJECT_DIR$/.idea/fleet-gitops.iml" />
    </modules>
  </component>
</project>
"#;

    fs::write(idea_dir.join("modules.xml"), modules_xml)?;
    println!("    ✓ modules.xml");

    // Generate fleet-gitops.iml
    let iml = r#"<?xml version="1.0" encoding="UTF-8"?>
<module type="WEB_MODULE" version="4">
  <component name="NewModuleRootManager" inherit-compiler-output="true">
    <exclude-output />
    <content url="file://$MODULE_DIR$">
      <sourceFolder url="file://$MODULE_DIR$/lib" isTestSource="false" />
      <sourceFolder url="file://$MODULE_DIR$/teams" isTestSource="false" />
    </content>
    <orderEntry type="sourceFolder" forTests="false" />
  </component>
</module>
"#;

    fs::write(idea_dir.join("fleet-gitops.iml"), iml)?;
    println!("    ✓ fleet-gitops.iml");

    Ok(())
}

fn generate_readme(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating README...");

    let readme = r#"# Fleet GitOps - IntelliJ IDEA Configuration

Auto-generated IntelliJ IDEA configuration for Fleet GitOps YAML editing.

## Features

- ✅ JSON Schema validation for all Fleet YAML files
- ✅ Auto-completion with descriptions and examples
- ✅ Live templates (code snippets) for policies, queries, labels
- ✅ File templates for creating new Fleet configurations
- ✅ Project structure recognition

## Installation

### Method 1: Copy to Project

Copy the generated files to your Fleet GitOps project:

```bash
# Copy .idea directory
cp -r intellij/.idea /path/to/your/fleet-gitops-project/

# Copy schemas
cp -r intellij/schemas /path/to/your/fleet-gitops-project/intellij/
```

### Method 2: Import Live Templates

1. Open IntelliJ IDEA
2. Go to **File → Manage IDE Settings → Import Settings**
3. Select `templates/Fleet-GitOps.xml`
4. Check "Live templates"
5. Click OK

### Method 3: Manual Setup

1. Open your Fleet GitOps project in IntelliJ IDEA
2. Go to **Preferences → Languages & Frameworks → Schemas and DTDs → JSON Schema Mappings**
3. Click **+** to add a new schema mapping
4. For each schema:
   - **Name**: Fleet Default Configuration
   - **Schema file or URL**: Browse to `intellij/schemas/fleet-default.schema.json`
   - **Schema version**: JSON Schema version 7
   - Add file path pattern: `default.yml`

## Usage

### Auto-completion

Start typing in any Fleet YAML file, and IntelliJ will suggest:
- Field names
- Enum values (platform, logging, etc.)
- Example values

Press **Ctrl+Space** to trigger completion manually.

### Live Templates

Type the abbreviation and press **Tab**:

- `fleet-policy` - Create a new policy
- `fleet-query` - Create a new query
- `fleet-label` - Create a new label
- `fleet-control-macos` - Create macOS settings control
- `fleet-software-package` - Create software package definition

### File Templates

**Right-click** in Project view → **New** → Select:
- **Fleet Policy** - Creates a new policy YAML file
- **Fleet Query** - Creates a new query YAML file
- **Fleet Team** - Creates a new team configuration file

### Schema Validation

IntelliJ will automatically validate your YAML files and show errors inline:
- Red underlines for errors
- Yellow for warnings
- Hover to see error details

### Quick Documentation

Hover over any field to see:
- Description
- Examples
- Valid values (enums)
- Type information

Or press **F1** (or **Ctrl+J** on Windows/Linux) with cursor on a field.

## Keyboard Shortcuts

| Action | macOS | Windows/Linux |
|--------|-------|---------------|
| Auto-complete | ⌃Space | Ctrl+Space |
| Quick documentation | F1 | Ctrl+Q |
| Parameter info | ⌘P | Ctrl+P |
| Show intention actions | ⌥↩ | Alt+Enter |
| Reformat code | ⌘⌥L | Ctrl+Alt+L |

## Schema Mappings

The following schema mappings are configured:

| Schema | File Pattern |
|--------|-------------|
| Fleet Default Configuration | `default.yml`, `default.yaml` |
| Fleet Team Configuration | `teams/*.yml`, `teams/*.yaml` |
| Fleet Policy | `lib/policies/*.yml` |
| Fleet Query | `lib/queries/*.yml` |
| Fleet Label | `lib/labels/*.yml` |

## Troubleshooting

### Schema validation not working

1. Go to **Preferences → Languages & Frameworks → Schemas and DTDs → JSON Schema Mappings**
2. Check that schemas are mapped correctly
3. Restart IntelliJ IDEA

### Auto-completion not showing

1. Make sure YAML plugin is installed and enabled
2. Check **Preferences → Editor → General → Code Completion**
3. Ensure "Autopopup code completion" is enabled

### Live templates not available

1. Go to **Preferences → Editor → Live Templates**
2. Look for "Fleet GitOps" group
3. If missing, import from `templates/Fleet-GitOps.xml`

## Customization

### Adding Custom Templates

1. Go to **Preferences → Editor → Live Templates**
2. Select "Fleet GitOps" group (or create it)
3. Click **+** → **Live Template**
4. Enter abbreviation, description, and template text
5. Set applicable context to "YAML"

### Modifying Schemas

Edit the schema files in `intellij/schemas/` to add custom fields or descriptions.

After modifying, reload schemas:
1. **Help → Find Action** (⌘⇧A / Ctrl+Shift+A)
2. Type "Reload All from Disk"
3. Press Enter

## Integration with Other Tools

### fleetctl

IntelliJ schemas complement fleetctl:
- **IntelliJ**: Edit-time validation and autocomplete
- **fleetctl**: Runtime validation and deployment

### Git

Add to `.gitignore` if you don't want to commit IntelliJ files:
```
.idea/
*.iml
```

Or commit `.idea/jsonSchemas.xml` to share schema mappings with your team.

## Generated by fleet-schema-gen

This configuration was automatically generated. To regenerate:

```bash
fleet-schema-gen generate --editor intellij --output .
```

## More Information

- [IntelliJ JSON Schema Support](https://www.jetbrains.com/help/idea/json.html#ws_json_schema_add_custom)
- [Live Templates Documentation](https://www.jetbrains.com/help/idea/using-live-templates.html)
- [Fleet GitOps Documentation](https://fleetdm.com/docs/configuration/yaml-files)
"#;

    fs::write(output_dir.join("README.md"), readme)?;
    println!("    ✓ README.md");

    Ok(())
}
