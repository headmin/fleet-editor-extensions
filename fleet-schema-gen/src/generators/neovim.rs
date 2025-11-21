use anyhow::Result;
use std::path::Path;
use std::fs;
use serde_json::json;
use crate::schema::types::FleetSchema;

pub fn generate(schema: &FleetSchema, output_dir: &Path) -> Result<()> {
    println!("\n=== Generating Neovim Configuration ===");

    fs::create_dir_all(output_dir)?;

    // 1. Generate JSON schemas
    generate_schemas(schema, output_dir)?;

    // 2. Generate LSP configuration (for nvim-lspconfig)
    generate_lsp_config(output_dir)?;

    // 3. Generate schema store integration
    generate_schemastore_config(output_dir)?;

    // 4. Generate snippets (LuaSnip format)
    generate_luasnip_snippets(output_dir)?;

    // 5. Generate snippets (UltiSnips format)
    generate_ultisnips_snippets(output_dir)?;

    // 6. Generate snippets (vim-snippets/SnipMate format)
    generate_snipmate_snippets(output_dir)?;

    // 7. Generate coc.nvim configuration
    generate_coc_config(output_dir)?;

    // 8. Generate README
    generate_readme(output_dir)?;

    println!("✓ Neovim configuration generated at: {}", output_dir.display());

    Ok(())
}

fn generate_schemas(schema: &FleetSchema, output_dir: &Path) -> Result<()> {
    println!("\n  → Generating JSON schemas...");

    let schemas_dir = output_dir.join("schemas");
    fs::create_dir_all(&schemas_dir)?;

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
        output_schema.schema = Some("http://json-schema.org/draft-07/schema#".to_string());

        let json = serde_json::to_string_pretty(&output_schema)?;
        fs::write(schemas_dir.join(filename), json)?;

        println!("    ✓ {}", filename);
    }

    Ok(())
}

fn generate_lsp_config(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating LSP configuration (nvim-lspconfig)...");

    // Lua configuration for yamlls with JSON schemas
    let lsp_config = r#"-- Fleet GitOps YAML LSP Configuration
-- Add this to your init.lua or yaml-lsp.lua

local lspconfig = require('lspconfig')

lspconfig.yamlls.setup {
  settings = {
    yaml = {
      validate = true,
      hover = true,
      completion = true,
      format = {
        enable = true,
      },
      schemas = {
        ["./neovim/schemas/fleet-default.schema.json"] = { "default.yml", "default.yaml" },
        ["./neovim/schemas/fleet-team.schema.json"] = { "teams/*.yml", "teams/*.yaml" },
        ["./neovim/schemas/fleet-policy.schema.json"] = { "lib/policies/*.yml", "lib/policies/*.yaml" },
        ["./neovim/schemas/fleet-query.schema.json"] = { "lib/queries/*.yml", "lib/queries/*.yaml" },
        ["./neovim/schemas/fleet-label.schema.json"] = { "lib/labels/*.yml", "lib/labels/*.yaml" },
      },
      schemaStore = {
        enable = true,
        url = "https://www.schemastore.org/api/json/catalog.json",
      },
    },
  },
}

-- Alternative: Use absolute paths if relative paths don't work
-- local project_root = vim.fn.getcwd()
-- local schemas = {
--   [project_root .. "/neovim/schemas/fleet-default.schema.json"] = { "default.yml" },
--   -- ... etc
-- }
"#;

    fs::write(output_dir.join("lspconfig.lua"), lsp_config)?;
    println!("    ✓ lspconfig.lua");

    Ok(())
}

fn generate_schemastore_config(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating schemastore configuration...");

    // Configuration for vim-yaml-schemastore or b0o/schemastore.nvim
    let schemastore_config = r#"-- Fleet GitOps Schema Store Configuration
-- For use with b0o/schemastore.nvim

local schemastore = require('schemastore')

require('lspconfig').yamlls.setup {
  settings = {
    yaml = {
      schemaStore = {
        enable = false, -- Disable default schema store
        url = "",
      },
      schemas = vim.tbl_extend(
        'force',
        schemastore.yaml.schemas(),
        {
          ["./neovim/schemas/fleet-default.schema.json"] = { "default.yml", "default.yaml" },
          ["./neovim/schemas/fleet-team.schema.json"] = { "teams/*.yml", "teams/*.yaml" },
          ["./neovim/schemas/fleet-policy.schema.json"] = { "lib/policies/*.yml" },
          ["./neovim/schemas/fleet-query.schema.json"] = { "lib/queries/*.yml" },
          ["./neovim/schemas/fleet-label.schema.json"] = { "lib/labels/*.yml" },
        }
      ),
    },
  },
}
"#;

    fs::write(output_dir.join("schemastore.lua"), schemastore_config)?;
    println!("    ✓ schemastore.lua");

    Ok(())
}

fn generate_luasnip_snippets(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating LuaSnip snippets...");

    let snippets_dir = output_dir.join("luasnip");
    fs::create_dir_all(&snippets_dir)?;

    let luasnip = r#"-- Fleet GitOps LuaSnip snippets
-- Place in: ~/.config/nvim/luasnip/yaml.lua
-- Or load with: require("luasnip.loaders.from_lua").load({paths = "./neovim/luasnip"})

local ls = require("luasnip")
local s = ls.snippet
local t = ls.text_node
local i = ls.insert_node
local c = ls.choice_node

return {
  -- Fleet Policy snippet
  s("fleet-policy", {
    t("- name: \""), i(1, "Platform"), t(" - "), i(2, "Check name"), t("\""), t({"", "  description: \""}), i(3, "Policy description"), t("\""),
    t({"", "  query: \""}), i(4, "SELECT 1 FROM table WHERE condition;"), t("\""),
    t({"", "  platform: \""}), c(5, {t("darwin"), t("windows"), t("linux"), t("chrome")}), t("\""),
    t({"", "  critical: "}), c(6, {t("false"), t("true")}),
  }),

  -- Fleet Query snippet
  s("fleet-query", {
    t("- name: \""), i(1, "query_name"), t("\""),
    t({"", "  query: \""}), i(2, "SELECT * FROM table;"), t("\""),
    t({"", "  description: \""}), i(3, "Query description"), t("\""),
    t({"", "  interval: "}), i(4, "3600"),
    t({"", "  platform: \""}), c(5, {t("darwin"), t("windows"), t("linux"), t("chrome")}), t("\""),
  }),

  -- Fleet Label snippet
  s("fleet-label", {
    t("- name: \""), i(1, "Label name"), t("\""),
    t({"", "  query: \""}), i(2, "SELECT 1 FROM system_info WHERE condition;"), t("\""),
    t({"", "  description: \""}), i(3, "Label description"), t("\""),
  }),

  -- Fleet macOS Control snippet
  s("fleet-control-macos", {
    t("macos_settings:"),
    t({"", "  custom_settings:"}),
    t({"", "    - path: \""}), i(1, "./lib/macos/profiles/security.mobileconfig"), t("\""),
    t({"", "      labels:"}),
    t({"", "        - \""}), i(2, "macOS laptops"), t("\""),
  }),

  -- Fleet Software Package snippet
  s("fleet-software", {
    t("- software:"),
    t({"", "    - url: \""}), i(1, "https://example.com/package.pkg"), t("\""),
    t({"", "      install_script: \""}), i(2, "./lib/scripts/install.sh"), t("\""),
    t({"", "      uninstall_script: \""}), i(3, "./lib/scripts/uninstall.sh"), t("\""),
    t({"", "      pre_install_query: \""}), i(4, "SELECT 1;"), t("\""),
  }),
}
"#;

    fs::write(snippets_dir.join("yaml.lua"), luasnip)?;
    println!("    ✓ yaml.lua (LuaSnip)");

    Ok(())
}

fn generate_ultisnips_snippets(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating UltiSnips snippets...");

    let snippets_dir = output_dir.join("UltiSnips");
    fs::create_dir_all(&snippets_dir)?;

    let ultisnips = r#"# Fleet GitOps UltiSnips
# Place in: ~/.config/nvim/UltiSnips/yaml.snippets

snippet fleet-policy "Fleet Policy" b
- name: "${1:Platform} - ${2:Check name}"
  description: "${3:Policy description}"
  query: "${4:SELECT 1 FROM table WHERE condition;}"
  platform: "${5:darwin}"
  critical: ${6:false}
endsnippet

snippet fleet-query "Fleet Query" b
- name: "${1:query_name}"
  query: "${2:SELECT * FROM table;}"
  description: "${3:Query description}"
  interval: ${4:3600}
  platform: "${5:darwin}"
endsnippet

snippet fleet-label "Fleet Label" b
- name: "${1:Label name}"
  query: "${2:SELECT 1 FROM system_info WHERE condition;}"
  description: "${3:Label description}"
endsnippet

snippet fleet-control-macos "Fleet macOS Control" b
macos_settings:
  custom_settings:
    - path: "${1:./lib/macos/profiles/security.mobileconfig}"
      labels:
        - "${2:macOS laptops}"
endsnippet

snippet fleet-software "Fleet Software Package" b
- software:
    - url: "${1:https://example.com/package.pkg}"
      install_script: "${2:./lib/scripts/install.sh}"
      uninstall_script: "${3:./lib/scripts/uninstall.sh}"
      pre_install_query: "${4:SELECT 1;}"
endsnippet

snippet fleet-default "Fleet Default YAML" b
---
apiVersion: v1
kind: default
spec:
  # Policies
  policies:
    - name: "${1:Policy name}"
      description: "${2:Description}"
      query: "${3:SELECT 1;}"
      platform: "${4:darwin}"

  # Queries
  queries:
    - name: "${5:query_name}"
      query: "${6:SELECT * FROM table;}"
      interval: 3600

  # Labels
  labels:
    - name: "${7:Label name}"
      query: "${8:SELECT 1;}"
endsnippet
"#;

    fs::write(snippets_dir.join("yaml.snippets"), ultisnips)?;
    println!("    ✓ yaml.snippets (UltiSnips)");

    Ok(())
}

fn generate_snipmate_snippets(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating SnipMate snippets...");

    let snippets_dir = output_dir.join("snippets");
    fs::create_dir_all(&snippets_dir)?;

    let snipmate = r#"# Fleet GitOps SnipMate snippets
# Place in: ~/.config/nvim/snippets/yaml.snippets

snippet fleet-policy
	- name: "${1:Platform} - ${2:Check name}"
	  description: "${3:Policy description}"
	  query: "${4:SELECT 1 FROM table WHERE condition;}"
	  platform: "${5:darwin}"
	  critical: ${6:false}

snippet fleet-query
	- name: "${1:query_name}"
	  query: "${2:SELECT * FROM table;}"
	  description: "${3:Query description}"
	  interval: ${4:3600}
	  platform: "${5:darwin}"

snippet fleet-label
	- name: "${1:Label name}"
	  query: "${2:SELECT 1 FROM system_info WHERE condition;}"
	  description: "${3:Label description}"
"#;

    fs::write(snippets_dir.join("yaml.snippets"), snipmate)?;
    println!("    ✓ yaml.snippets (SnipMate)");

    Ok(())
}

fn generate_coc_config(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating coc.nvim configuration...");

    // coc-settings.json for coc.nvim users
    let coc_settings = json!({
        "yaml.schemas": {
            "./neovim/schemas/fleet-default.schema.json": ["default.yml", "default.yaml"],
            "./neovim/schemas/fleet-team.schema.json": ["teams/*.yml", "teams/*.yaml"],
            "./neovim/schemas/fleet-policy.schema.json": ["lib/policies/*.yml", "lib/policies/*.yaml"],
            "./neovim/schemas/fleet-query.schema.json": ["lib/queries/*.yml", "lib/queries/*.yaml"],
            "./neovim/schemas/fleet-label.schema.json": ["lib/labels/*.yml", "lib/labels/*.yaml"]
        },
        "yaml.validate": true,
        "yaml.completion": true,
        "yaml.hover": true,
        "yaml.format.enable": true
    });

    let coc_json = serde_json::to_string_pretty(&coc_settings)?;
    fs::write(output_dir.join("coc-settings.json"), coc_json)?;

    println!("    ✓ coc-settings.json");

    Ok(())
}

fn generate_readme(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating README...");

    let readme = r#"# Fleet GitOps - Neovim Configuration

Auto-generated Neovim configuration for Fleet GitOps YAML editing with LSP support.

## Features

- ✅ JSON Schema validation via yaml-language-server (yamlls)
- ✅ Auto-completion with descriptions and examples
- ✅ Hover documentation
- ✅ Snippets in multiple formats (LuaSnip, UltiSnips, SnipMate)
- ✅ Support for multiple LSP clients (nvim-lspconfig, coc.nvim)

## Prerequisites

### Required

- **Neovim 0.8+** (for LSP support)
- **yaml-language-server** (yamlls)
  ```bash
  npm install -g yaml-language-server
  ```

### Plugin Manager

Choose one:
- [lazy.nvim](https://github.com/folke/lazy.nvim) (recommended)
- [packer.nvim](https://github.com/wbthomason/packer.nvim)
- [vim-plug](https://github.com/junegunn/vim-plug)

### LSP Client

Choose one:
- **nvim-lspconfig** (recommended)
- **coc.nvim**

### Snippet Engine (Optional)

Choose one:
- **LuaSnip** (recommended)
- **UltiSnips**
- **vim-snippets** (SnipMate)

## Installation

### Method 1: nvim-lspconfig (Recommended)

#### Step 1: Install Plugins

With **lazy.nvim**:
```lua
-- ~/.config/nvim/lua/plugins/lsp.lua
return {
  {
    'neovim/nvim-lspconfig',
    dependencies = {
      'hrsh7th/cmp-nvim-lsp',  -- Completion source
    },
  },
  {
    'L3MON4D3/LuaSnip',  -- Snippet engine (optional)
    build = "make install_jsregexp",
  },
}
```

#### Step 2: Copy Schema Files

```bash
# Copy schemas to your project
cp -r neovim/schemas /path/to/your/fleet-gitops-project/neovim/
```

#### Step 3: Configure LSP

Add to your `init.lua` or create `~/.config/nvim/after/ftplugin/yaml.lua`:

```lua
-- Option A: Copy from lspconfig.lua
local lspconfig = require('lspconfig')

lspconfig.yamlls.setup {
  settings = {
    yaml = {
      validate = true,
      hover = true,
      completion = true,
      schemas = {
        ["./neovim/schemas/fleet-default.schema.json"] = { "default.yml", "default.yaml" },
        ["./neovim/schemas/fleet-team.schema.json"] = { "teams/*.yml", "teams/*.yaml" },
        ["./neovim/schemas/fleet-policy.schema.json"] = { "lib/policies/*.yml" },
        ["./neovim/schemas/fleet-query.schema.json"] = { "lib/queries/*.yml" },
        ["./neovim/schemas/fleet-label.schema.json"] = { "lib/labels/*.yml" },
      },
    },
  },
}
```

#### Step 4: Install Snippets (Optional)

**For LuaSnip:**
```bash
mkdir -p ~/.config/nvim/luasnip
cp neovim/luasnip/yaml.lua ~/.config/nvim/luasnip/
```

Add to your config:
```lua
require("luasnip.loaders.from_lua").load({paths = "~/.config/nvim/luasnip"})
```

**For UltiSnips:**
```bash
mkdir -p ~/.config/nvim/UltiSnips
cp neovim/UltiSnips/yaml.snippets ~/.config/nvim/UltiSnips/
```

### Method 2: coc.nvim

#### Step 1: Install coc.nvim and coc-yaml

```vim
" In your init.vim
Plug 'neoclide/coc.nvim', {'branch': 'release'}
```

Then install coc-yaml:
```vim
:CocInstall coc-yaml
```

#### Step 2: Copy Configuration

```bash
# Copy schemas
cp -r neovim/schemas /path/to/your/fleet-gitops-project/neovim/

# Copy coc settings
cp neovim/coc-settings.json /path/to/your/fleet-gitops-project/
```

Or merge into your existing `coc-settings.json`:
```bash
cat neovim/coc-settings.json >> ~/.config/nvim/coc-settings.json
```

### Method 3: Minimal Setup (No LSP)

If you just want snippets without LSP:

```bash
# For UltiSnips
mkdir -p ~/.config/nvim/UltiSnips
cp neovim/UltiSnips/yaml.snippets ~/.config/nvim/UltiSnips/

# For SnipMate
mkdir -p ~/.config/nvim/snippets
cp neovim/snippets/yaml.snippets ~/.config/nvim/snippets/
```

## Usage

### Auto-completion

Type in any Fleet YAML file and completion will appear automatically.

**Trigger manually:**
- nvim-lspconfig: `<C-Space>` or `<C-x><C-o>`
- coc.nvim: `<Tab>` or as configured

### Hover Documentation

Place cursor on any field and press:
- `K` (default hover key)
- Or `:lua vim.lsp.buf.hover()`

### Snippets

Type the trigger and press your snippet expand key:

**LuaSnip:** (typically `<Tab>` or `<C-k>`)
```
fleet-policy<Tab>
fleet-query<Tab>
fleet-label<Tab>
```

**UltiSnips:** (typically `<Tab>`)
```
fleet-policy<Tab>
fleet-query<Tab>
fleet-label<Tab>
```

Available snippets:
- `fleet-policy` - Create a policy
- `fleet-query` - Create a query
- `fleet-label` - Create a label
- `fleet-control-macos` - macOS settings control
- `fleet-software` - Software package definition

### Validation

Errors and warnings will appear inline:
- **Red underline** - Errors
- **Yellow underline** - Warnings
- **Signs in gutter** - Diagnostic indicators

Show diagnostics:
```vim
:lua vim.diagnostic.open_float()
```

Or configure to show automatically:
```lua
vim.diagnostic.config({
  virtual_text = true,
  signs = true,
  update_in_insert = false,
  underline = true,
  severity_sort = true,
  float = {
    border = 'rounded',
    source = 'always',
  },
})
```

## Configuration Examples

### Full LSP Setup with Keybindings

```lua
-- ~/.config/nvim/after/ftplugin/yaml.lua

local lspconfig = require('lspconfig')

-- Keybindings
local on_attach = function(client, bufnr)
  local bufopts = { noremap=true, silent=true, buffer=bufnr }
  vim.keymap.set('n', 'gd', vim.lsp.buf.definition, bufopts)
  vim.keymap.set('n', 'K', vim.lsp.buf.hover, bufopts)
  vim.keymap.set('n', 'gi', vim.lsp.buf.implementation, bufopts)
  vim.keymap.set('n', '<C-k>', vim.lsp.buf.signature_help, bufopts)
  vim.keymap.set('n', '<space>rn', vim.lsp.buf.rename, bufopts)
  vim.keymap.set('n', '<space>ca', vim.lsp.buf.code_action, bufopts)
  vim.keymap.set('n', 'gr', vim.lsp.buf.references, bufopts)
  vim.keymap.set('n', '<space>f', function()
    vim.lsp.buf.format { async = true }
  end, bufopts)
end

-- Setup yamlls
lspconfig.yamlls.setup {
  on_attach = on_attach,
  capabilities = require('cmp_nvim_lsp').default_capabilities(),
  settings = {
    yaml = {
      validate = true,
      hover = true,
      completion = true,
      format = { enable = true },
      schemas = {
        ["./neovim/schemas/fleet-default.schema.json"] = { "default.yml" },
        ["./neovim/schemas/fleet-team.schema.json"] = { "teams/*.yml" },
        ["./neovim/schemas/fleet-policy.schema.json"] = { "lib/policies/*.yml" },
        ["./neovim/schemas/fleet-query.schema.json"] = { "lib/queries/*.yml" },
        ["./neovim/schemas/fleet-label.schema.json"] = { "lib/labels/*.yml" },
      },
    },
  },
}
```

### With nvim-cmp Completion

```lua
local cmp = require('cmp')
local luasnip = require('luasnip')

cmp.setup({
  snippet = {
    expand = function(args)
      luasnip.lsp_expand(args.body)
    end,
  },
  mapping = cmp.mapping.preset.insert({
    ['<C-Space>'] = cmp.mapping.complete(),
    ['<CR>'] = cmp.mapping.confirm({ select = true }),
    ['<Tab>'] = cmp.mapping(function(fallback)
      if cmp.visible() then
        cmp.select_next_item()
      elseif luasnip.expand_or_jumpable() then
        luasnip.expand_or_jump()
      else
        fallback()
      end
    end, { 'i', 's' }),
  }),
  sources = {
    { name = 'nvim_lsp' },
    { name = 'luasnip' },
    { name = 'buffer' },
    { name = 'path' },
  },
})
```

## Troubleshooting

### Schema validation not working

**Check yamlls is running:**
```vim
:LspInfo
```

Should show `yamlls` attached to buffer.

**Check schema paths:**
```vim
:lua vim.print(vim.lsp.get_active_clients()[1].config.settings.yaml.schemas)
```

If paths are wrong, use absolute paths:
```lua
local root = vim.fn.getcwd()
schemas = {
  [root .. "/neovim/schemas/fleet-default.schema.json"] = { "default.yml" },
}
```

### Auto-completion not appearing

1. Ensure nvim-cmp or completion plugin is installed
2. Check LSP is attached: `:LspInfo`
3. Try manual trigger: `<C-x><C-o>` (omni-completion)

### Snippets not working

**LuaSnip:**
```lua
-- Check snippets are loaded
:lua print(vim.inspect(require("luasnip").get_snippets("yaml")))
```

**UltiSnips:**
```vim
" Check UltiSnips is working
:UltiSnipsEdit
```

### Hover documentation not showing

Try:
```vim
:lua vim.lsp.buf.hover()
```

If nothing appears, LSP may not be attached or field may not have documentation.

## Integration with Other Tools

### With Telescope

Find schema files:
```lua
require('telescope.builtin').find_files({
  prompt_title = "Fleet Schemas",
  cwd = vim.fn.getcwd() .. "/neovim/schemas",
})
```

### With null-ls (Formatting/Linting)

```lua
local null_ls = require("null-ls")

null_ls.setup({
  sources = {
    null_ls.builtins.formatting.prettier.with({
      filetypes = { "yaml" },
    }),
    null_ls.builtins.diagnostics.yamllint,
  },
})
```

### With Trouble.nvim (Diagnostics)

```lua
require("trouble").setup {}

-- Show diagnostics
vim.keymap.set("n", "<leader>xx", "<cmd>Trouble<cr>")
vim.keymap.set("n", "<leader>xd", "<cmd>Trouble document_diagnostics<cr>")
```

## File Structure

```
neovim/
├── schemas/
│   ├── fleet-default.schema.json
│   ├── fleet-team.schema.json
│   ├── fleet-policy.schema.json
│   ├── fleet-query.schema.json
│   └── fleet-label.schema.json
├── luasnip/
│   └── yaml.lua                    # LuaSnip snippets
├── UltiSnips/
│   └── yaml.snippets               # UltiSnips snippets
├── snippets/
│   └── yaml.snippets               # SnipMate snippets
├── lspconfig.lua                   # nvim-lspconfig setup
├── schemastore.lua                 # Schema store integration
├── coc-settings.json               # coc.nvim configuration
└── README.md                       # This file
```

## Generated by fleet-schema-gen

This configuration was automatically generated.

To regenerate:
```bash
fleet-schema-gen generate --editor neovim --output .
```

## Resources

- [Neovim LSP Documentation](https://neovim.io/doc/user/lsp.html)
- [nvim-lspconfig](https://github.com/neovim/nvim-lspconfig)
- [yaml-language-server](https://github.com/redhat-developer/yaml-language-server)
- [LuaSnip](https://github.com/L3MON4D3/LuaSnip)
- [Fleet GitOps Documentation](https://fleetdm.com/docs/configuration/yaml-files)
"#;

    fs::write(output_dir.join("README.md"), readme)?;
    println!("    ✓ README.md");

    Ok(())
}
