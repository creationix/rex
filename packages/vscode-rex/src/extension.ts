import * as vscode from "vscode";
import { TOKEN_TYPES, tokenize, type Token } from "./rexc-tokenizer";

const ANNOTATION_TYPE = TOKEN_TYPES.indexOf("annotation");

const legend = new vscode.SemanticTokensLegend(TOKEN_TYPES as unknown as string[]);

const annotationDecoration = vscode.window.createTextEditorDecorationType({
	fontStyle: "normal",
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
		const builder = new vscode.SemanticTokensBuilder(legend);
		const blocks = tokenizeDocument(document);

		for (const block of blocks) {
			for (const token of block.tokens) {
				// Skip annotations — let TextMate handle color,
				// decoration handles fontStyle override
				if (token.type === ANNOTATION_TYPE) continue;
				builder.push(
					token.line + block.lineOffset,
					token.char,
					token.length,
					token.type,
					0,
				);
			}
		}

		return builder.build();
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
	const provider = new RexcSemanticTokenProvider();
	context.subscriptions.push(
		vscode.languages.registerDocumentSemanticTokensProvider(
			{ language: "rexc" },
			provider,
			legend,
		),
		vscode.languages.registerDocumentSemanticTokensProvider(
			{ language: "markdown" },
			provider,
			legend,
		),
	);

	// Apply non-italic decoration to annotation ranges
	if (vscode.window.activeTextEditor) {
		updateAnnotationDecorations(vscode.window.activeTextEditor);
	}
	context.subscriptions.push(
		vscode.window.onDidChangeActiveTextEditor((editor) => {
			if (editor) updateAnnotationDecorations(editor);
		}),
		vscode.workspace.onDidChangeTextDocument((e) => {
			const editor = vscode.window.activeTextEditor;
			if (editor && editor.document === e.document) {
				updateAnnotationDecorations(editor);
			}
		}),
	);
}
