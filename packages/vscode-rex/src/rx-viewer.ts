import * as vscode from "vscode";
import { basename } from "node:path";

class RxDocument implements vscode.CustomDocument {
	constructor(readonly uri: vscode.Uri) {}
	dispose() {}
}

export class RxViewerProvider implements vscode.CustomReadonlyEditorProvider<RxDocument> {
	static readonly viewType = "rex.rxViewer";

	constructor(private readonly extensionUri: vscode.Uri) {}

	static register(context: vscode.ExtensionContext): vscode.Disposable {
		const provider = new RxViewerProvider(context.extensionUri);
		return vscode.window.registerCustomEditorProvider(
			RxViewerProvider.viewType,
			provider,
			{
				webviewOptions: { retainContextWhenHidden: true },
				supportsMultipleEditorsPerDocument: false,
			},
		);
	}

	openCustomDocument(uri: vscode.Uri): RxDocument {
		return new RxDocument(uri);
	}

	async resolveCustomEditor(
		document: RxDocument,
		webviewPanel: vscode.WebviewPanel,
	): Promise<void> {
		const webview = webviewPanel.webview;
		webview.options = {
			enableScripts: true,
			localResourceRoots: [vscode.Uri.joinPath(this.extensionUri, "dist", "webview")],
		};

		webview.html = this.getHtml(webview);

		const sendContent = async () => {
			const raw = await vscode.workspace.fs.readFile(document.uri);
			const text = Buffer.from(raw).toString("utf8");
			webview.postMessage({
				type: "load",
				content: text,
				name: basename(document.uri.fsPath),
			});
		};

		// Send content once the webview signals it's ready
		const messageDisposable = webview.onDidReceiveMessage((msg) => {
			if (msg.type === "ready") {
				sendContent();
			}
		});

		// Watch for file changes on disk
		const watcher = vscode.workspace.createFileSystemWatcher(
			new vscode.RelativePattern(document.uri, "*"),
		);
		const onChange = () => sendContent();
		watcher.onDidChange(onChange);

		webviewPanel.onDidDispose(() => {
			messageDisposable.dispose();
			watcher.dispose();
		});
	}

	private getHtml(webview: vscode.Webview): string {
		const webviewDir = vscode.Uri.joinPath(this.extensionUri, "dist", "webview");
		const scriptUri = webview.asWebviewUri(vscode.Uri.joinPath(webviewDir, "webview.js"));
		const styleUri = webview.asWebviewUri(vscode.Uri.joinPath(webviewDir, "webview.css"));
		const nonce = getNonce();

		return `<!DOCTYPE html>
<html lang="en" class="dark">
<head>
	<meta charset="UTF-8">
	<meta name="viewport" content="width=device-width, initial-scale=1.0">
	<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}' blob:; worker-src blob:; font-src ${webview.cspSource};">
	<link rel="stylesheet" href="${styleUri}">
	<title>RX Viewer</title>
	<style nonce="${nonce}">
		/* Remap app color tokens to VS Code theme */
		:root {
			/* Backgrounds */
			--color-bg: var(--vscode-editor-background);
			--color-bg-secondary: var(--vscode-sideBar-background, var(--vscode-editor-background));
			--color-bg-tertiary: var(--vscode-editorGroupHeader-tabsBackground, var(--vscode-editor-background));
			--color-bg-hover: var(--vscode-list-hoverBackground);
			--color-bg-deep: var(--vscode-input-background);
			--color-bg-toolbar: var(--vscode-titleBar-activeBackground, var(--vscode-editor-background));
			--color-bg-active: var(--vscode-tab-activeBackground, var(--vscode-editor-background));
			--color-bg-selection: var(--vscode-editor-selectionBackground);
			--color-bg-error: var(--vscode-inputValidation-errorBackground);
			--color-bg-overlay: rgba(0, 0, 0, 0.5);
			--color-bg-row-even: var(--vscode-list-hoverBackground);
			--color-bg-row-odd: var(--vscode-editor-background);

			/* Borders */
			--color-border: var(--vscode-panel-border, var(--vscode-widget-border));
			--color-border-subtle: var(--vscode-widget-border, var(--vscode-panel-border));
			--color-border-focus: var(--vscode-focusBorder);
			--color-border-error: var(--vscode-inputValidation-errorBorder);

			/* Text */
			--color-text: var(--vscode-editor-foreground);
			--color-text-bright: var(--vscode-foreground);
			--color-text-dim: var(--vscode-descriptionForeground);
			--color-text-muted: var(--vscode-disabledForeground);
			--color-text-label: var(--vscode-disabledForeground);
			--color-text-placeholder: var(--vscode-input-placeholderForeground);
			--color-text-inverse: var(--vscode-button-foreground, white);

			/* Semantic */
			--color-accent: var(--vscode-textLink-foreground);
			--color-error: var(--vscode-errorForeground);

			/* Buttons */
			--color-btn-primary-bg: var(--vscode-button-background);
			--color-btn-primary-text: var(--vscode-button-foreground);
			--color-btn-primary-hover: var(--vscode-button-hoverBackground);

			/* REXC syntax colors — use editor token colors where available */
			--color-rexc-string: var(--vscode-debugTokenExpression-string, #ce9178);
			--color-rexc-number: var(--vscode-debugTokenExpression-number, #b5cea8);
			--color-rexc-boolean: var(--vscode-debugTokenExpression-boolean, #569cd6);
			--color-rexc-object: var(--vscode-symbolIcon-objectForeground, #dcdcaa);
			--color-rexc-variable: var(--vscode-symbolIcon-variableForeground, #9cdcfe);

			/* Tag colors — theme colors brightened for visual pop */
			--color-tag-ref: color-mix(in oklch, var(--vscode-symbolIcon-referenceForeground, #fb7676) 60%, white);
			--color-tag-string: color-mix(in oklch, var(--vscode-debugTokenExpression-string, #fbb676) 60%, white);
			--color-tag-integer: color-mix(in oklch, var(--vscode-debugTokenExpression-number, #b6fb76) 60%, white);
			--color-tag-decimal: color-mix(in oklch, var(--vscode-debugTokenExpression-number, #76fb76) 60%, white);
			--color-tag-key: color-mix(in oklch, var(--vscode-symbolIcon-propertyForeground, #76fbb6) 60%, white);
			--color-tag-chain: color-mix(in oklch, var(--vscode-symbolIcon-fieldForeground, #76b6fb) 60%, white);
			--color-tag-object: color-mix(in oklch, var(--vscode-symbolIcon-objectForeground, #7676fb) 60%, white);
			--color-tag-pointer: color-mix(in oklch, var(--vscode-debugTokenExpression-boolean, #b676fb) 60%, white);
			--color-tag-array: color-mix(in oklch, var(--vscode-symbolIcon-arrayForeground, #fb76b6) 60%, white);
			--color-tag-index: color-mix(in oklch, var(--vscode-descriptionForeground, #9c9c9c) 60%, white);

			color-scheme: var(--vscode-colorScheme, dark);
		}
		html, body, #app {
			background: var(--vscode-editor-background);
			color: var(--vscode-editor-foreground);
			font-family: var(--vscode-font-family, system-ui, sans-serif);
			font-size: var(--vscode-font-size, 13px);
		}
		* {
			scrollbar-color: var(--vscode-scrollbarSlider-background) transparent;
		}
	</style>
</head>
<body>
	<div id="app"></div>
	<script nonce="${nonce}" src="${scriptUri}"></script>
</body>
</html>`;
	}
}

function getNonce(): string {
	let text = "";
	const possible = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
	for (let i = 0; i < 32; i++) {
		text += possible.charAt(Math.floor(Math.random() * possible.length));
	}
	return text;
}
