import * as vscode from "vscode";
import { dirname, join } from "node:path";
import { getRexParseFailure } from "./rex-diagnostics";
import {
	type RexDomainSchema,
	entryToDetail,
	parseDomainSchema,
	resolveDomainPrefixMatches,
	resolveDomainPath,
} from "./rex-domain";
import {
	analyzeRexSymbols,
	findDefinitionAtOffset,
	findReferencesAtOffset,
} from "./rex-symbols";
import { RxViewerProvider } from "./rx-viewer";

const REX_TOKEN_TYPES = ["rexLocal", "rexDomain"];
const REX_TOKEN_MODIFIERS = ["declaration"];
const REX_LOCAL_TYPE = 0;
const REX_DOMAIN_TYPE = 1;
const REX_DECLARATION_MODIFIER = 1 << 0;
const rexLegend = new vscode.SemanticTokensLegend(REX_TOKEN_TYPES, REX_TOKEN_MODIFIERS);

class RexSemanticTokenProvider
	implements vscode.DocumentSemanticTokensProvider
{
	constructor(private readonly readSchema: (document: vscode.TextDocument) => Promise<RexDomainSchema | null>) {}

	async provideDocumentSemanticTokens(
		document: vscode.TextDocument,
	): Promise<vscode.SemanticTokens> {
		const source = document.getText();
		const analysis = analyzeRexSymbols(source);
		const schema = await this.readSchema(document);
		const builder = new vscode.SemanticTokensBuilder(rexLegend);

		for (const definition of analysis.definitions) {
			const start = document.positionAt(definition.start);
			builder.push(
				start.line,
				start.character,
				definition.end - definition.start,
				REX_LOCAL_TYPE,
				REX_DECLARATION_MODIFIER,
			);
		}

		for (const reference of analysis.references) {
			const start = document.positionAt(reference.start);
			const resolved = findDefinitionAtOffset(source, reference.start);
			if (resolved) {
				builder.push(
					start.line,
					start.character,
					reference.end - reference.start,
					REX_LOCAL_TYPE,
					0,
				);
				continue;
			}

			if (resolveDomainPath(schema ?? {}, [reference.name])) {
				builder.push(
					start.line,
					start.character,
					reference.end - reference.start,
					REX_DOMAIN_TYPE,
					0,
				);
			}
		}

		return builder.build();
	}
}

class RexDocumentSymbolProvider implements vscode.DocumentSymbolProvider {
	provideDocumentSymbols(document: vscode.TextDocument): vscode.DocumentSymbol[] {
		const analysis = analyzeRexSymbols(document.getText());
		return analysis.definitions.map((definition) => {
			const start = document.positionAt(definition.start);
			const end = document.positionAt(definition.end);
			const range = new vscode.Range(start, end);
			const symbol = new vscode.DocumentSymbol(
				definition.name,
				definition.kind,
				vscode.SymbolKind.Variable,
				range,
				range,
			);
			return symbol;
		});
	}
}

class RexDefinitionProvider implements vscode.DefinitionProvider {
	provideDefinition(
		document: vscode.TextDocument,
		position: vscode.Position,
	): vscode.Definition | null {
		const offset = document.offsetAt(position);
		const target = findDefinitionAtOffset(document.getText(), offset);
		if (!target) return null;

		const range = new vscode.Range(
			document.positionAt(target.start),
			document.positionAt(target.end),
		);
		return new vscode.Location(document.uri, range);
	}
}

class RexReferenceProvider implements vscode.ReferenceProvider {
	provideReferences(
		document: vscode.TextDocument,
		position: vscode.Position,
		context: vscode.ReferenceContext,
	): vscode.Location[] {
		const offset = document.offsetAt(position);
		const locations = findReferencesAtOffset(
			document.getText(),
			offset,
			context.includeDeclaration,
		);

		return locations.map((location) =>
			new vscode.Location(
				document.uri,
				new vscode.Range(
					document.positionAt(location.start),
					document.positionAt(location.end),
				),
			),
		);
	}
}

class RexCompletionProvider implements vscode.CompletionItemProvider {
	constructor(private readonly readSchema: (document: vscode.TextDocument) => Promise<RexDomainSchema | null>) {}

	async provideCompletionItems(
		document: vscode.TextDocument,
		position: vscode.Position,
	): Promise<vscode.CompletionItem[]> {
		const schema = await this.readSchema(document);
		if (!schema?.globals) return [];

		const toCompletionItems = (
			entries: Record<string, { description?: string; type?: string; properties?: Record<string, unknown> }>,
			kind: vscode.CompletionItemKind,
		): vscode.CompletionItem[] => {
			const items: vscode.CompletionItem[] = [];
			for (const [name, entry] of Object.entries(entries)) {
				const item = new vscode.CompletionItem(name, kind);
				item.detail = entryToDetail(entry);
				item.documentation = entry.description;
				item.sortText = `0_${name}`;
				items.push(item);
			}
			return items;
		};

		const line = document.lineAt(position.line).text;
		const prefix = line.slice(0, position.character);
		const chainMatch = prefix.match(/([A-Za-z_][A-Za-z0-9_-]*(?:\.[A-Za-z_][A-Za-z0-9_-]*)*)\.[A-Za-z0-9_-]*$/);

		if (chainMatch) {
			const chain = chainMatch[1]?.split(".") ?? [];
			const target = resolveDomainPath(schema, chain);
			if (target?.properties) {
				return toCompletionItems(target.properties, vscode.CompletionItemKind.Field);
			}
			const prefix = chain.join(".");
			const matches = resolveDomainPrefixMatches(schema, prefix);
			if (Object.keys(matches).length > 0) {
				return toCompletionItems(matches, vscode.CompletionItemKind.Field);
			}
			return [];
		}

		return toCompletionItems(schema.globals, vscode.CompletionItemKind.Variable);
	}
}

class RexHoverProvider implements vscode.HoverProvider {
	constructor(private readonly readSchema: (document: vscode.TextDocument) => Promise<RexDomainSchema | null>) {}

	async provideHover(
		document: vscode.TextDocument,
		position: vscode.Position,
	): Promise<vscode.Hover | null> {
		const schema = await this.readSchema(document);
		if (!schema) return null;

		const line = document.lineAt(position.line).text;
		const left = line.slice(0, position.character + 1);
		const match = left.match(/([A-Za-z_][A-Za-z0-9_-]*(?:\.[A-Za-z_][A-Za-z0-9_-]*)*)$/);
		if (!match?.[1]) return null;

		const entry = resolveDomainPath(schema, match[1].split("."));
		if (!entry) return null;

		const markdown = new vscode.MarkdownString();
		markdown.appendCodeblock(entryToDetail(entry), "text");
		if (entry.description) {
			markdown.appendMarkdown(`\n\n${entry.description}`);
		}
		return new vscode.Hover(markdown);
	}
}

export function activate(context: vscode.ExtensionContext) {
	const rexDiagnostics = vscode.languages.createDiagnosticCollection("rex");
	context.subscriptions.push(rexDiagnostics);
	type CachedDomainSchema = { schema: RexDomainSchema | null; mtime: number };
	const schemaCache = new Map<string, CachedDomainSchema>();

	async function findNearestDomainSchemaFile(document: vscode.TextDocument): Promise<vscode.Uri | null> {
		if (document.uri.scheme === "file") {
			const workspaceFolder = vscode.workspace.getWorkspaceFolder(document.uri);
			const workspaceRoot = workspaceFolder?.uri.fsPath;
			let current = dirname(document.uri.fsPath);
			while (true) {
				const candidate = vscode.Uri.file(join(current, ".config.rex"));
				try {
					await vscode.workspace.fs.stat(candidate);
					return candidate;
				} catch {
					// Continue walking up
				}

				if (workspaceRoot) {
					if (current === workspaceRoot) break;
					if (!current.startsWith(workspaceRoot)) break;
				}

				const parent = dirname(current);
				if (parent === current) break;
				current = parent;
			}
		}

		const files = await vscode.workspace.findFiles(".config.rex", "**/node_modules/**", 1);
		return files[0] ?? null;
	}

	async function readDomainSchema(document: vscode.TextDocument): Promise<RexDomainSchema | null> {
		const file = await findNearestDomainSchemaFile(document);
		if (!file) return null;

		try {
			const stat = await vscode.workspace.fs.stat(file);
			const cacheKey = file.toString();
			const cached = schemaCache.get(cacheKey);
			if (cached && cached.mtime === stat.mtime) return cached.schema;

			const raw = await vscode.workspace.fs.readFile(file);
			const parsed = parseDomainSchema(Buffer.from(raw).toString("utf8"));
			schemaCache.set(cacheKey, { schema: parsed, mtime: stat.mtime });
			return parsed;
		} catch {
			schemaCache.delete(file.toString());
			return null;
		}
	}

	function updateRexDiagnostics(document: vscode.TextDocument) {
		if (document.languageId !== "rex") {
			rexDiagnostics.delete(document.uri);
			return;
		}

		const failure = getRexParseFailure(document.getText());
		if (!failure) {
			rexDiagnostics.delete(document.uri);
			return;
		}

		const start = document.positionAt(failure.startOffset);
		const end = document.positionAt(failure.endOffset);
		const range = new vscode.Range(start, end);
		const diagnostic = new vscode.Diagnostic(
			range,
			failure.message,
			vscode.DiagnosticSeverity.Error,
		);
		diagnostic.source = "rex";
		rexDiagnostics.set(document.uri, [diagnostic]);
	}

	// Rex language providers
	const rexSemanticProvider = new RexSemanticTokenProvider(readDomainSchema);
	const rexSymbols = new RexDocumentSymbolProvider();
	const rexDefinitions = new RexDefinitionProvider();
	const rexReferences = new RexReferenceProvider();
	const rexCompletions = new RexCompletionProvider(readDomainSchema);
	const rexHover = new RexHoverProvider(readDomainSchema);
	context.subscriptions.push(
		vscode.languages.registerDocumentSymbolProvider({ language: "rex" }, rexSymbols),
		vscode.languages.registerDefinitionProvider({ language: "rex" }, rexDefinitions),
		vscode.languages.registerReferenceProvider({ language: "rex" }, rexReferences),
		vscode.languages.registerCompletionItemProvider(
			{ language: "rex" },
			rexCompletions,
			".",
		),
		vscode.languages.registerHoverProvider({ language: "rex" }, rexHover),
		vscode.languages.registerDocumentSemanticTokensProvider(
			{ language: "rex" },
			rexSemanticProvider,
			rexLegend,
		),
	);

	// RX/REXC custom viewer
	context.subscriptions.push(
		RxViewerProvider.register(context),
	);

	// Rex diagnostics
	for (const document of vscode.workspace.textDocuments) {
		updateRexDiagnostics(document);
	}

	context.subscriptions.push(
		vscode.workspace.onDidOpenTextDocument(updateRexDiagnostics),
		vscode.window.onDidChangeActiveTextEditor((editor) => {
			if (!editor) return;
			updateRexDiagnostics(editor.document);
		}),
		vscode.workspace.onDidChangeTextDocument((e) => {
			updateRexDiagnostics(e.document);
		}),
	);
}
