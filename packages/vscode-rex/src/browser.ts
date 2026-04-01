import { LanguageClient } from "vscode-languageclient/browser";
import * as vscode from "vscode";

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext) {
	const serverModule = vscode.Uri.joinPath(
		context.extensionUri,
		"dist/server-browser.js",
	);

	const worker = new Worker(serverModule.toString(true));

	const config = vscode.workspace.getConfiguration("rex");

	client = new LanguageClient(
		"rex",
		"Rex Language Server (Web)",
		{
			documentSelector: [
				{ language: "rex" },
				{ language: "rexd" },
			],
			initializationOptions: {
				extensionUri: context.extensionUri.toString(),
				domain: config.get<string>("domainFile") || undefined,
			},
		},
		worker,
	);

	context.subscriptions.push(client);
	client.start();

	// Note: RxViewerProvider uses Node.js APIs and is NOT available in the web extension.
}

export async function deactivate(): Promise<void> {
	if (client) {
		await client.stop();
		client = undefined;
	}
}
