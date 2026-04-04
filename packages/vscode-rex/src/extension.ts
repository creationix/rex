import * as vscode from "vscode";
import {
	LanguageClient,
	type LanguageClientOptions,
	type ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext) {
	// Register rext semantic tokens first — doesn't depend on rex CLI
	const legend = new vscode.SemanticTokensLegend(
		["keyword", "operator", "string", "number", "variable", "property", "type"],
		[]
	);
	const rextProvider = new RextSemanticTokensProvider(legend);
	context.subscriptions.push(
		vscode.languages.registerDocumentSemanticTokensProvider(
			{ language: "rext" }, rextProvider, legend
		),
		vscode.languages.registerDocumentSemanticTokensProvider(
			{ language: "rx" }, rextProvider, legend
		),
		vscode.languages.registerDocumentSemanticTokensProvider(
			{ language: "markdown" }, new MarkdownRextProvider(rextProvider), legend
		),
	);

	// Start Rex LSP (requires rex CLI)
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

// ── Rext bytecode semantic tokenizer ─────────────────────────────────

const B64 = new Set("0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_");
const B64_CHARS = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_";

const enum Tok { keyword, operator, string, number, variable, property, type }

class RextSemanticTokensProvider implements vscode.DocumentSemanticTokensProvider {
	public legend: vscode.SemanticTokensLegend;
	constructor(legend: vscode.SemanticTokensLegend) { this.legend = legend; }

	provideDocumentSemanticTokens(doc: vscode.TextDocument): vscode.SemanticTokens {
		const builder = new vscode.SemanticTokensBuilder(this.legend);
		const lines: string[] = [];
		for (let i = 0; i < doc.lineCount; i++) lines.push(doc.lineAt(i).text);
		this.tokenizeBlock(lines, 0, builder);
		return builder.build();
	}

	/** Tokenize a contiguous block of rext lines. Tracks string-body and container state across lines. */
	tokenizeBlock(lines: string[], startLine: number, builder: vscode.SemanticTokensBuilder) {
		let stringRemaining = 0; // UTF-8 bytes left in a multi-line string body
		let stringIsKey = false; // is the multi-line string in key position?
		let indexTableRemaining = 0; // b64 digits left in an index pointer table
		const stack: { isObject: boolean; isKey: boolean }[] = [];

		const inKeyPos = () => {
			const top = stack[stack.length - 1];
			return top !== undefined && top.isObject && top.isKey;
		};
		const afterValue = () => {
			const top = stack[stack.length - 1];
			if (top !== undefined && top.isObject) top.isKey = !top.isKey;
		};

		for (let li = 0; li < lines.length; li++) {
			const line = lines[li]!;
			const absLine = startLine + li;
			let col = 0;

			// Continue a string body from a previous line
			if (stringRemaining > 0) {
				const { chars, bytes } = countUtf8(line, 0, stringRemaining);
				if (chars > 0) builder.push(absLine, 0, chars, stringIsKey ? Tok.property : Tok.string);
				col += chars;
				stringRemaining -= bytes;
				if (stringRemaining > 0) {
					stringRemaining--; // newline = 1 byte
					continue;
				}
				afterValue();
			}

			// Continue an index pointer table from a previous line
			if (indexTableRemaining > 0) {
				const tableStart = col;
				while (col < line.length && indexTableRemaining > 0 && B64.has(line[col]!)) {
					col++;
					indexTableRemaining--;
				}
				if (col > tableStart) builder.push(absLine, tableStart, col - tableStart, Tok.type);
			}

			let varintStart = -1; // start column of current b64 digit run

			while (col < line.length) {
				const ch = line[col]!;

				if (B64.has(ch)) {
					if (varintStart < 0) varintStart = col;
					col++;
					continue;
				}

				if (ch === " " || ch === "\t") {
					varintStart = -1;
					col++;
					continue;
				}

				// We hit a tag character. Determine span including any preceding varint.
				const vs = varintStart;
				varintStart = -1;
				const spanStart = vs >= 0 ? vs : col;
				const spanLen = col - spanStart + 1;
				const isKey = inKeyPos();

				switch (ch) {
					// ── Scalars with varint ──────────────────────────
					case "+":   // integer
					case "*":   // decimal (exponent prefix)
						builder.push(absLine, spanStart, spanLen, isKey ? Tok.property : Tok.number);
						col++; afterValue(); break;

					case "^":   // pointer (forward delta)
						builder.push(absLine, spanStart, spanLen, isKey ? Tok.property : Tok.type);
						col++; afterValue(); break;

					case ",": { // string — varint = byte length, then raw body
						const utf8Len = parseB64(vs >= 0 ? line.slice(vs, col) : "");
						builder.push(absLine, spanStart, spanLen, isKey ? Tok.property : Tok.string);
						col++; // past ','
						if (utf8Len > 0) {
							const { chars, bytes } = countUtf8(line, col, utf8Len);
							if (chars > 0) builder.push(absLine, col, chars, isKey ? Tok.property : Tok.string);
							col += chars;
							if (bytes < utf8Len) {
								stringRemaining = utf8Len - bytes - 1;
								stringIsKey = isKey;
							} else {
								afterValue();
							}
						} else {
							afterValue();
						}
						break;
					}

					case "'":   // ref (name in varint: true→t', none→no')
						builder.push(absLine, spanStart, spanLen, isKey ? Tok.property : Tok.variable);
						col++; afterValue(); break;

					case "$":   // variable (name in varint)
						builder.push(absLine, spanStart, spanLen, isKey ? Tok.property : Tok.variable);
						col++; afterValue(); break;

					case "%":   // opcode (mnemonic in varint)
						builder.push(absLine, spanStart, spanLen, isKey ? Tok.property : Tok.keyword);
						col++; afterValue(); break;

					case "\\":  // break/continue
						builder.push(absLine, spanStart, spanLen, isKey ? Tok.property : Tok.keyword);
						col++; afterValue(); break;

					// ── Modifiers (no varint, not values) ────────────
					case "?": case "&": case "|": case ">": case "<":
						builder.push(absLine, col, 1, Tok.keyword);
						col++; break;

					case ";":   // return
						builder.push(absLine, col, 1, Tok.keyword);
						col++; break;

					// ── Index / while (varint = packed header, not a value) ──
					case "#": {
						builder.push(absLine, spanStart, spanLen, Tok.keyword);
						col++;
						// Skip pointer table: count * width b64 digits
						const packed = parseB64(vs >= 0 ? line.slice(vs, col - 1) : "");
						const count = packed >> 3;
						const width = (packed & 7) + 1;
						let tableSize = count * width;
						const tableStart = col;
						while (col < line.length && tableSize > 0 && B64.has(line[col]!)) {
							col++;
							tableSize--;
						}
						if (col > tableStart) builder.push(absLine, tableStart, col - tableStart, Tok.type);
						indexTableRemaining = tableSize;
						break;
					}

					// ── Operators (not values) ──────────────────────
					case "=": case "/": case "~":
						builder.push(absLine, col, 1, Tok.operator);
						col++; break;

					case ".":   // chain (varint = byte count)
						builder.push(absLine, spanStart, spanLen, Tok.operator);
						col++; break;

					// ── Container delimiters — no semantic token, let bracket pair colorization handle these
					case "{":
						stack.push({ isObject: true, isKey: true });
						col++; break;
					case "[":
						stack.push({ isObject: false, isKey: false });
						col++; break;
					case "(":
						stack.push({ isObject: false, isKey: false });
						col++; break;
					case "}": case "]": case ")":
						stack.pop();
						col++; afterValue(); break;

					default:
						col++; break;
				}
			}
		}
	}
}

/** Count JS chars consumed and UTF-8 bytes used, up to maxBytes. */
function countUtf8(line: string, start: number, maxBytes: number): { chars: number; bytes: number } {
	let bytes = 0, chars = 0, i = start;
	while (i < line.length && bytes < maxBytes) {
		const cp = line.codePointAt(i)!;
		const b = cp <= 0x7f ? 1 : cp <= 0x7ff ? 2 : cp <= 0xffff ? 3 : 4;
		if (bytes + b > maxBytes) break;
		bytes += b;
		if (cp > 0xffff) { chars += 2; i += 2; } else { chars++; i++; }
	}
	return { chars, bytes };
}

function parseB64(s: string): number {
	let n = 0;
	for (const ch of s) {
		const v = B64_CHARS.indexOf(ch);
		if (v < 0) break;
		n = n * 64 + v;
	}
	return n;
}

class MarkdownRextProvider implements vscode.DocumentSemanticTokensProvider {
	constructor(private inner: RextSemanticTokensProvider) {}

	provideDocumentSemanticTokens(doc: vscode.TextDocument): vscode.SemanticTokens {
		const builder = new vscode.SemanticTokensBuilder(this.inner.legend);
		let inRext = false;
		let blockLines: string[] = [];
		let blockStart = 0;

		for (let lineIdx = 0; lineIdx < doc.lineCount; lineIdx++) {
			const line = doc.lineAt(lineIdx).text;
			if (inRext) {
				if (line.startsWith("```")) {
					if (blockLines.length > 0) {
						this.inner.tokenizeBlock(blockLines, blockStart, builder);
					}
					blockLines = [];
					inRext = false;
				} else {
					blockLines.push(line);
				}
			} else if (/^```rext\s*$/.test(line)) {
				inRext = true;
				blockStart = lineIdx + 1;
			}
		}

		return builder.build();
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
				const parent = dirname(dir);
			if (parent === dir) break;
			dir = parent;
		}
	}

	return undefined;
}
