import * as vscode from "vscode";
import {
	LanguageClient,
	type LanguageClientOptions,
	type ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext) {
	const rexPath = findRexBinary();
	if (!rexPath) {
		vscode.window.showWarningMessage(
			"Rex CLI not found. Install rex and ensure it is on PATH, or set rex.path in settings.",
		);
		return;
	}

	const serverOptions: ServerOptions = {
		command: rexPath,
		args: ["lsp"],
	};

	const config = vscode.workspace.getConfiguration("rex");
	const clientOptions: LanguageClientOptions = {
		documentSelector: [
			{ scheme: "file", language: "rex" },
		],
		initializationOptions: {
			domain: config.get<string>("domainFile") || undefined,
		},
	};

	client = new LanguageClient(
		"rex",
		"Rex Language Server",
		serverOptions,
		clientOptions,
	);

	client.start();
}

export async function deactivate(): Promise<void> {
	if (client) {
		await client.stop();
		client = undefined;
	}
}

function findRexBinary(): string | undefined {
	const { existsSync } = require("fs");
	const { join, dirname } = require("path");

	// 1. Check rex.path setting
	const config = vscode.workspace.getConfiguration("rex");
	const configPath = config.get<string>("path");
	if (configPath) return configPath;

	// 2. Check PATH (VS Code may not inherit shell PATH)
	const { execSync } = require("child_process");
	try {
		const shell = process.env.SHELL || "/bin/sh";
		execSync(`${shell} -lc "rex --version"`, { stdio: "ignore" });
		return "rex";
	} catch {}

	// 3. Dev mode: search upward from each workspace folder for target/release/rex or target/debug/rex
	for (const folder of vscode.workspace.workspaceFolders ?? []) {
		let dir = folder.uri.fsPath;
		while (true) {
			const releasePath = join(dir, "target/release/rex");
			if (existsSync(releasePath)) return releasePath;
			const debugPath = join(dir, "target/debug/rex");
			if (existsSync(debugPath)) return debugPath;
			// Also check packages/rusty-rex/ subdirectory
			const pkgRelease = join(dir, "packages/rusty-rex/target/release/rex");
			if (existsSync(pkgRelease)) return pkgRelease;
			const pkgDebug = join(dir, "packages/rusty-rex/target/debug/rex");
			if (existsSync(pkgDebug)) return pkgDebug;
			const parent = dirname(dir);
			if (parent === dir) break;
			dir = parent;
		}
	}

	return undefined;
}
