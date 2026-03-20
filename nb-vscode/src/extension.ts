import * as path from 'path';
import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext): void {
    // 查找 nb-lsp 可执行文件：
    // 优先使用配置中指定的路径，否则从 PATH 中查找
    const config = vscode.workspace.getConfiguration('nb');
    const serverPath: string = config.get('lspPath') || 'nb-lsp';

    const serverOptions: ServerOptions = {
        command: serverPath,
        transport: TransportKind.stdio,
    };

    const clientOptions: LanguageClientOptions = {
        // 只对 .nb 文件激活
        documentSelector: [{ scheme: 'file', language: 'nb' }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.nb'),
        },
    };

    client = new LanguageClient(
        'nb-lsp',
        'NB Language Server',
        serverOptions,
        clientOptions,
    );

    client.start();
    context.subscriptions.push(client);
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
