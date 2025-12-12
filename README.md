# Fleet GitOps VS Code Extension

JSON Schema validation and autocomplete for Fleet GitOps YAML files.

## Features

- Real-time validation and error detection
- Intelligent autocomplete for Fleet YAML fields
- Code snippets for common patterns
- LSP server with diagnostics and quick-fixes

## Installation

Download the `.vsix` from [Releases](https://github.com/headmin/fleetctl-vscode/releases) and install:

```bash
code --install-extension fleet-gitops-*.vsix
```

## Editor Support

### VS Code
Install the extension directly. Requires the [Red Hat YAML extension](https://marketplace.visualstudio.com/items?itemName=redhat.vscode-yaml).

### Sublime Text
Use [LSP](https://packagecontrol.io/packages/LSP) + [LSP-yaml](https://packagecontrol.io/packages/LSP-yaml) with the generated schemas, or run `fleet-schema-gen` as an LSP server.

### Other Editors
Any editor supporting YAML Language Server or custom LSP servers can use the generated schemas.

## Linting

The extension validates:
- Field names and typos
- Value types (string, boolean, integer)
- Required fields
- Enum values (`platform`, `logging`, etc.)
- Format patterns (URLs, SHA256 hashes)

Run `fleetctl gitops --dry-run` for server-side validation.

## Development

See [DEVELOPMENT.md](DEVELOPMENT.md) for build instructions.

## License

Apache 2.0
