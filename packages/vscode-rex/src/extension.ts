import * as vscode from "vscode";
import { TOKEN_TYPES, tokenize, type Token } from "./rexc-tokenizer";
import { getRexParseFailure } from "./rex-diagnostics";
import {
	type RexDomainSchema,
	entryToDetail,
	parseDomainSchema,
	resolveDomainPath,
} from "./rex-domain";
import {
	analyzeRexSymbols,
	findDefinitionAtOffset,
	findReferencesAtOffset,
} from "./rex-symbols";

const ANNOTATION_TYPE = TOKEN_TYPES.indexOf("annotation");
const BYTE_LENGTH_TYPE = TOKEN_TYPES.indexOf("byteLength");
const NUMBER_TYPE = TOKEN_TYPES.indexOf("number");

const rexcLegend = new vscode.SemanticTokensLegend(TOKEN_TYPES as unknown as string[]);
const REX_TOKEN_TYPES = ["rexLocal", "rexDomain"];
const REX_TOKEN_MODIFIERS = ["declaration"];
const REX_LOCAL_TYPE = 0;
const REX_DOMAIN_TYPE = 1;
const REX_DECLARATION_MODIFIER = 1 << 0;
const rexLegend = new vscode.SemanticTokensLegend(REX_TOKEN_TYPES, REX_TOKEN_MODIFIERS);

const annotationDecoration = vscode.window.createTextEditorDecorationType({
	fontStyle: "normal",
	color: new vscode.ThemeColor("editorLineNumber.foreground"),
});

const REXC_FENCE = /^```rexc\s*$/gm;
const FENCE_CLOSE = /^```\s*$/gm;

interface Block {
	tokens: Token[];
	lineOffset: number;
}

function tokenizeDocument(document: vscode.TextDocument): Block[] {
	const text = document.getText();

	if (document.languageId === "rexc") {
		return [{ tokens: tokenize(text), lineOffset: 0 }];
	}

	// Markdown: find ```rexc blocks
	const blocks: Block[] = [];
	REXC_FENCE.lastIndex = 0;
	let open;
	while ((open = REXC_FENCE.exec(text)) !== null) {
		const contentStart = open.index + open[0].length + 1;
		FENCE_CLOSE.lastIndex = contentStart;
		const close = FENCE_CLOSE.exec(text);
		if (!close) break;
		const blockText = text.slice(contentStart, close.index);
		const startLine = document.positionAt(contentStart).line;
		blocks.push({ tokens: tokenize(blockText), lineOffset: startLine });
		REXC_FENCE.lastIndex = close.index + close[0].length;
	}
	return blocks;
}

class RexcSemanticTokenProvider
	implements vscode.DocumentSemanticTokensProvider
{
	provideDocumentSemanticTokens(
		document: vscode.TextDocument,
	): vscode.SemanticTokens {
		const builder = new vscode.SemanticTokensBuilder(rexcLegend);
		const blocks = tokenizeDocument(document);

		for (const block of blocks) {
			for (const token of block.tokens) {
				// Skip annotations — decoration handles both color and fontStyle
				if (token.type === ANNOTATION_TYPE) continue;
				const type =
					token.type === BYTE_LENGTH_TYPE ? NUMBER_TYPE : token.type;
				builder.push(
					token.line + block.lineOffset,
					token.char,
					token.length,
					type,
					0,
				);
			}
		}

		return builder.build();
	}
}

class RexSemanticTokenProvider
	implements vscode.DocumentSemanticTokensProvider
{
	constructor(private readonly readSchema: () => Promise<RexDomainSchema | null>) {}

	async provideDocumentSemanticTokens(
		document: vscode.TextDocument,
	): Promise<vscode.SemanticTokens> {
		const source = document.getText();
		const analysis = analyzeRexSymbols(source);
		const schema = await this.readSchema();
		const globals = schema?.globals ?? {};
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

			if (globals[reference.name]) {
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
	constructor(private readonly readSchema: () => Promise<RexDomainSchema | null>) {}

	async provideCompletionItems(
		document: vscode.TextDocument,
		position: vscode.Position,
	): Promise<vscode.CompletionItem[]> {
		const schema = await this.readSchema();
		if (!schema?.globals) return [];

		const line = document.lineAt(position.line).text;
		const prefix = line.slice(0, position.character);
		const chainMatch = prefix.match(/([A-Za-z_][A-Za-z0-9_-]*(?:\.[A-Za-z_][A-Za-z0-9_-]*)*)\.[A-Za-z0-9_-]*$/);

		if (chainMatch) {
			const chain = chainMatch[1]?.split(".") ?? [];
			const target = resolveDomainPath(schema, chain);
			if (!target?.properties) return [];
			return Object.entries(target.properties).map(([name, entry]) => {
				const item = new vscode.CompletionItem(name, vscode.CompletionItemKind.Field);
				item.detail = entryToDetail(entry);
				item.documentation = entry.description;
				return item;
			});
		}

		return Object.entries(schema.globals).map(([name, entry]) => {
			const item = new vscode.CompletionItem(name, vscode.CompletionItemKind.Variable);
			item.detail = entryToDetail(entry);
			item.documentation = entry.description;
			return item;
		});
	}
}

class RexHoverProvider implements vscode.HoverProvider {
	constructor(private readonly readSchema: () => Promise<RexDomainSchema | null>) {}

	async provideHover(
		document: vscode.TextDocument,
		position: vscode.Position,
	): Promise<vscode.Hover | null> {
		const schema = await this.readSchema();
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

function updateAnnotationDecorations(editor: vscode.TextEditor) {
	const blocks = tokenizeDocument(editor.document);
	const ranges: vscode.Range[] = [];

	for (const block of blocks) {
		for (const token of block.tokens) {
			if (token.type === ANNOTATION_TYPE) {
				const line = token.line + block.lineOffset;
				ranges.push(
					new vscode.Range(line, token.char, line, token.char + token.length),
				);
			}
		}
	}

	editor.setDecorations(annotationDecoration, ranges);
}

export function activate(context: vscode.ExtensionContext) {
	const rexDiagnostics = vscode.languages.createDiagnosticCollection("rex");
	context.subscriptions.push(rexDiagnostics);
	let cachedSchema: RexDomainSchema | null = null;
	let cachedSchemaMtime = -1;

	async function readDomainSchema(): Promise<RexDomainSchema | null> {
		const files = await vscode.workspace.findFiles("rex-domain.json", "**/node_modules/**", 1);
		const file = files[0];
		if (!file) {
			cachedSchema = null;
			cachedSchemaMtime = -1;
			return null;
		}

		try {
			const stat = await vscode.workspace.fs.stat(file);
			if (stat.mtime === cachedSchemaMtime && cachedSchema) return cachedSchema;
			const raw = await vscode.workspace.fs.readFile(file);
			const parsed = parseDomainSchema(Buffer.from(raw).toString("utf8"));
			cachedSchema = parsed;
			cachedSchemaMtime = stat.mtime;
			return parsed;
		} catch {
			cachedSchema = null;
			cachedSchemaMtime = -1;
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

	const provider = new RexcSemanticTokenProvider();
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
		vscode.languages.registerDocumentSemanticTokensProvider(
			{ language: "rexc" },
			provider,
			rexcLegend,
		),
		vscode.languages.registerDocumentSemanticTokensProvider(
			{ language: "markdown" },
			provider,
			rexcLegend,
		),
	);

	for (const document of vscode.workspace.textDocuments) {
		updateRexDiagnostics(document);
	}

	// Apply non-italic decoration to annotation ranges
	function updateAllVisible() {
		for (const editor of vscode.window.visibleTextEditors) {
			updateAnnotationDecorations(editor);
			updateRexDiagnostics(editor.document);
		}
	}
	updateAllVisible();
	// Re-apply after a short delay so decorations survive initial token processing
	setTimeout(updateAllVisible, 500);
	context.subscriptions.push(
		vscode.workspace.onDidOpenTextDocument(updateRexDiagnostics),
		vscode.window.onDidChangeActiveTextEditor((editor) => {
			if (!editor) return;
			updateAnnotationDecorations(editor);
			updateRexDiagnostics(editor.document);
		}),
		vscode.window.onDidChangeVisibleTextEditors(updateAllVisible),
		vscode.workspace.onDidChangeTextDocument((e) => {
			updateRexDiagnostics(e.document);
			for (const editor of vscode.window.visibleTextEditors) {
				if (editor.document === e.document) {
					updateAnnotationDecorations(editor);
				}
			}
		}),
	);
}
