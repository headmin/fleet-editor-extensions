import * as path from 'path';
import * as os from 'os';
import * as fs from 'fs';
import {
    workspace,
    ExtensionContext,
    window,
    commands,
    OutputChannel,
} from 'vscode';
import {
    Executable,
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;
let outputChannel: OutputChannel;

export async function activate(context: ExtensionContext): Promise<void> {
    outputChannel = window.createOutputChannel('Fleet GitOps');
    outputChannel.appendLine('Fleet GitOps extension activating...');

    // Check if extension is enabled
    const config = workspace.getConfiguration('fleetGitops');
    if (!config.get<boolean>('enable', true)) {
        outputChannel.appendLine('Fleet GitOps is disabled in settings');
        return;
    }

    // Get server path
    const serverPath = getServerPath(context, config);
    if (!serverPath) {
        window.showErrorMessage(
            'Fleet GitOps: Could not find fleet-schema-gen binary. Please set fleetGitops.serverPath in settings.'
        );
        return;
    }

    outputChannel.appendLine(`Using server binary: ${serverPath}`);

    // Verify binary exists
    if (!fs.existsSync(serverPath)) {
        window.showErrorMessage(
            `Fleet GitOps: Server binary not found at ${serverPath}`
        );
        return;
    }

    // Create server options (using Executable type like typos-lsp)
    const run: Executable = {
        command: serverPath,
        args: ['lsp'],
    };

    const debug: Executable = {
        command: serverPath,
        args: ['lsp', '--debug'],
    };

    const serverOptions: ServerOptions = {
        run,
        debug,
    };

    // Create client options
    const clientOptions: LanguageClientOptions = {
        // Register for YAML files matching Fleet GitOps patterns
        documentSelector: [
            { scheme: 'file', language: 'yaml', pattern: '**/default.yml' },
            { scheme: 'file', language: 'yaml', pattern: '**/default.yaml' },
            { scheme: 'file', language: 'yaml', pattern: '**/teams/**/*.yml' },
            { scheme: 'file', language: 'yaml', pattern: '**/teams/**/*.yaml' },
            { scheme: 'file', language: 'yaml', pattern: '**/lib/**/*.yml' },
            { scheme: 'file', language: 'yaml', pattern: '**/lib/**/*.yaml' },
        ],
        synchronize: {
            // Watch for changes to Fleet config files
            fileEvents: workspace.createFileSystemWatcher('**/*.{yml,yaml}'),
        },
        outputChannel,
        traceOutputChannel: outputChannel,
    };

    // Create the language client
    client = new LanguageClient(
        'fleetGitops',
        'Fleet GitOps',
        serverOptions,
        clientOptions
    );

    // Register commands
    context.subscriptions.push(
        commands.registerCommand('fleetGitops.restartServer', async () => {
            outputChannel.appendLine('Restarting Fleet LSP server...');
            if (client) {
                await client.restart();
                outputChannel.appendLine('Fleet LSP server restarted');
            }
        })
    );

    context.subscriptions.push(
        commands.registerCommand('fleetGitops.showOutput', () => {
            outputChannel.show();
        })
    );

    // Start the client
    try {
        await client.start();
        outputChannel.appendLine('Fleet GitOps LSP server started successfully');
    } catch (error) {
        outputChannel.appendLine(`Failed to start LSP server: ${error}`);
        window.showErrorMessage(
            `Fleet GitOps: Failed to start language server. Check the output channel for details.`
        );
    }

    context.subscriptions.push(client);
}

export async function deactivate(): Promise<void> {
    if (client) {
        await client.stop();
    }
}

/**
 * Get the path to the fleet-schema-gen binary.
 * Priority:
 * 1. User-configured path (fleetGitops.serverPath)
 * 2. Bundled binary in extension's bin/ directory
 */
function getServerPath(
    context: ExtensionContext,
    config: ReturnType<typeof workspace.getConfiguration>
): string | undefined {
    // Check user-configured path first
    const configuredPath = config.get<string>('serverPath');
    if (configuredPath && configuredPath.trim() !== '') {
        // Expand ~ to home directory
        const expandedPath = configuredPath.replace(/^~/, os.homedir());
        if (fs.existsSync(expandedPath)) {
            return expandedPath;
        }
        outputChannel.appendLine(
            `Configured server path not found: ${expandedPath}`
        );
    }

    // Try bundled binary
    const bundledPath = getBundledBinaryPath(context);
    if (bundledPath && fs.existsSync(bundledPath)) {
        return bundledPath;
    }

    // Try to find in PATH (for development)
    const pathBinary = findInPath('fleet-schema-gen');
    if (pathBinary) {
        return pathBinary;
    }

    return undefined;
}

/**
 * Get the path to the bundled binary for the current platform.
 */
function getBundledBinaryPath(context: ExtensionContext): string | undefined {
    const platform = os.platform();
    const arch = os.arch();

    let binaryName: string;
    switch (platform) {
        case 'darwin':
            binaryName = arch === 'arm64'
                ? 'fleet-schema-gen-darwin-arm64'
                : 'fleet-schema-gen-darwin-x64';
            break;
        case 'linux':
            binaryName = arch === 'arm64'
                ? 'fleet-schema-gen-linux-arm64'
                : 'fleet-schema-gen-linux-x64';
            break;
        case 'win32':
            binaryName = 'fleet-schema-gen-win32-x64.exe';
            break;
        default:
            outputChannel.appendLine(`Unsupported platform: ${platform}`);
            return undefined;
    }

    return path.join(context.extensionPath, 'bin', binaryName);
}

/**
 * Try to find an executable in the system PATH.
 */
function findInPath(name: string): string | undefined {
    const pathEnv = process.env.PATH || '';
    const pathSeparator = os.platform() === 'win32' ? ';' : ':';
    const extensions = os.platform() === 'win32' ? ['.exe', '.cmd', '.bat', ''] : [''];

    for (const dir of pathEnv.split(pathSeparator)) {
        for (const ext of extensions) {
            const fullPath = path.join(dir, name + ext);
            if (fs.existsSync(fullPath)) {
                return fullPath;
            }
        }
    }

    return undefined;
}
