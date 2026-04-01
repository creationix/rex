import {
	createConnection,
	TextDocuments,
	type Diagnostic,
	DiagnosticSeverity,
	BrowserMessageReader,
	BrowserMessageWriter,
	TextDocumentSyncKind,
	type InitializeResult,
} from "vscode-languageserver/browser";
import { TextDocument } from "vscode-languageserver-textdocument";

// WASM is loaded lazily; the extension host sends the base URI via initializationOptions
let wasmReady = false;

const messageReader = new BrowserMessageReader(self as any);
const messageWriter = new BrowserMessageWriter(self as any);
const connection = createConnection(messageReader, messageWriter);
const documents = new TextDocuments(TextDocument);

connection.onInitialize(async (params): Promise<InitializeResult> => {
	const baseUri = params.initializationOptions?.extensionUri as string;
	if (baseUri) {
		await loadWasm(baseUri);
		wasmReady = true;
	}
	return {
		capabilities: {
			textDocumentSync: TextDocumentSyncKind.Full,
			completionProvider: { triggerCharacters: ["."] },
			hoverProvider: true,
			definitionProvider: true,
		},
	};
});

documents.onDidChangeContent((change) => {
	if (!wasmReady) return;
	const diagnostics = computeDiagnostics(change.document);
	connection.sendDiagnostics({ uri: change.document.uri, diagnostics });
});

function computeDiagnostics(document: TextDocument): Diagnostic[] {
	const source = document.getText();
	const raw = (globalThis as any).wasm_bindgen?.check(source, "") ?? [];
	return raw.map((d: any) => ({
		range: {
			start: document.positionAt(d.start),
			end: document.positionAt(d.end),
		},
		message: d.message,
		severity:
			d.severity === "error"
				? DiagnosticSeverity.Error
				: DiagnosticSeverity.Warning,
		source: "rex",
	}));
}

async function loadWasm(baseUri: string): Promise<void> {
	(globalThis as any).importScripts(`${baseUri}/wasm/rex_wasm.js`);
	await (globalThis as any).wasm_bindgen(`${baseUri}/wasm/rex_wasm_bg.wasm`);
}

documents.listen(connection);
connection.listen();
