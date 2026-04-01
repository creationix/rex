# Instructions: Add LSP + MCP Support to the Rex CLI

## Goal

Add two new subcommands to the Rex CLI:

1. **`rex lsp`** — Language Server Protocol over stdio. Gives any LSP-capable editor (VS Code, Neovim, Zed, Helix, etc.) diagnostics, completions, hover, and go-to-definition powered by `rex-core`.
2. **`rex mcp`** — Model Context Protocol over stdio. Gives AI agents (OpenCode, Claude Desktop, Cursor, etc.) tool-calling access to Rex language tools: compile, check, format, evaluate.

Additionally, update the VS Code extension to:
- Replace the legacy TypeScript parser with an LSP client (desktop)
- Add a second `browser` entrypoint that runs the language intelligence inside a Web Worker using a new `rex-wasm` crate (VS Code for the Web / vscode.dev)

This covers all four target environments:

| Environment | How Rex intelligence runs |
|---|---|
| OpenCode / AI agents | `rex mcp` via stdio (MCP tool-calling) |
| Neovim / Helix / Zed | `rex lsp` via stdio (LSP JSON-RPC) |
| VS Code desktop | Extension spawns `rex lsp` via `vscode-languageclient/node` |
| VS Code Web (vscode.dev) | Extension loads `rex-wasm` in a Web Worker via `vscode-languageclient/browser` |

## Context

- The canonical Rex compiler, parser, and type checker are in **Rust** at `packages/rusty-rex/crates/rex-core`
- The CLI at `crates/rex-cli` already has a `check` subcommand that runs the type checker with `.rexd` auto-discovery
- The VS Code extension at `packages/vscode-rex` uses a **legacy TypeScript parser** that doesn't support template literals, `return`, variadic `and`/`or`, `type`/`extern` declarations, or the current bytecode format
- The type checker (`rex-core/src/typecheck.rs`) already validates Rex programs against `.rexd` domain interface files
- Domain interface files (`.rexd`) declare types, extern bindings, and function signatures — see `rex-types.md` for the full spec
- `rex-core` depends only on `logos` and `rowan` — both are pure Rust and compile cleanly to `wasm32-unknown-unknown` with no changes

## Architecture Overview

```
packages/rusty-rex/
  crates/
    rex-core/          # Pure Rust — parser, type checker, encoder. No OS deps.
    rex-cli/           # Builds the `rex` binary
      src/
        lsp/           # `rex lsp` subcommand (NEW)
        mcp/           # `rex mcp` subcommand (NEW)
    rex-wasm/          # NEW: rex-core compiled to WASM for browser/worker use
      src/lib.rs
      Cargo.toml

packages/vscode-rex/
  src/
    extension.ts       # Desktop entry (Node.js host): spawns `rex lsp`
    browser.ts         # Browser entry (Web Worker host): loads rex-wasm  (NEW)
    server-browser.ts  # Web Worker server-side LSP handler using rex-wasm (NEW)
    rx-viewer.ts       # Keep: custom RX/REXC viewer
```

---

## Part 1: `rex lsp` — Language Server Protocol

### Command definition (add to `crates/rex-cli/src/main.rs`)

```rust
/// Start the Language Server Protocol server over stdio
Lsp {
    /// Domain interface file (.rexd). Auto-discovered if not specified.
    #[arg(long)]
    domain: Option<PathBuf>,
},
```

### Dependencies to add to `crates/rex-cli/Cargo.toml`

```toml
lsp-server = "0.7"
lsp-types = "0.97"
serde_json = "1"
serde = { version = "1", features = ["derive"] }
```

### Module layout

```
crates/rex-cli/src/
  lsp/
    mod.rs          # Server entry, stdio transport, initialize/shutdown dispatch
    document.rs     # Open document state (HashMap<Url, String>)
    diagnostics.rs  # parse + typecheck → publish diagnostics on open/change/save
    completion.rs   # Completions from DomainSchema globals, functions, type aliases
    hover.rs        # Hover info from DomainSchema + inferred types
    definition.rs   # Go-to-definition → source location in .rexd
```

### How it's used

```sh
# Editors auto-discover and spawn it via their config (see below)
rex lsp

# Explicitly specify the domain interface file
rex lsp --domain ./server.rexd

# Verify it starts (pipe LSP initialize over stdin)
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{},"rootUri":null,"processId":null}}' | rex lsp
```

The server speaks LSP JSON-RPC 2.0 over stdin/stdout. `stderr` is available for debug logging. All editors and agents use the same binary.

### Capabilities

**Must implement:**
- `textDocument/publishDiagnostics` — parse + typecheck errors on open/change/save
- `textDocument/diagnostic` (pull model) — agents can request diagnostics on demand
- `textDocument/completion` — extern names, type names, object keys, keywords
- `textDocument/hover` — type and doc comment for identifiers
- `textDocument/definition` — jump to extern declaration in `.rexd`

**Nice to have:**
- `textDocument/documentSymbol` — outline (variable assignments, when/for blocks)
- `textDocument/formatting` — auto-format Rex source
- `textDocument/signatureHelp` — function parameter info for extern calls

### Document management

1. `textDocument/didOpen` — parse, find nearest `.rexd`, typecheck, publish diagnostics
2. `textDocument/didChange` — re-parse and re-check the changed content
3. `textDocument/didSave` — full re-check (in case `.rexd` changed on disk)

### Domain file discovery

Priority order:
1. `--domain` CLI flag
2. `initializationOptions.domain` from the LSP `initialize` request
3. Auto-discovery via `find_rexd` (already in `main.rs`) — searches upward from the open file for `*.rexd`

### Diagnostics implementation

```rust
// Parse errors
let (_, errors) = rex_core::parser::parse(source, &rex_core::lexer::lex(source));
// Typecheck errors
let schema = rex_core::typecheck::load_schema(&rexd_source)?;
let diagnostics = rex_core::typecheck::check_source(source, &schema);
// Map byte offsets to LSP line/col via offset_to_line_col (already in main.rs)
```

### Completions

Extract from `DomainSchema`:
1. `globals` — extern bindings (`req`, `res`, `headers`, etc.)
2. `functions` — extern functions (`json.parse`, `db.get`, etc.), triggered after `.`
3. `type_aliases` — type names in annotation positions
4. Object keys — from the expected type when cursor is inside `{`
5. Keywords — `when`, `unless`, `for`, `while`, `do`, `end`, `else`, `return`, `and`, `or`, `in`, `of`, `true`, `false`, `null`, `none`, `type`, `extern`, `mut`, `break`, `continue`, `delete`

### Neovim configuration

Users configure Neovim (via `nvim-lspconfig` or manual `vim.lsp.start`) like this. Document this in the extension README and/or a project-level `EDITOR-SETUP.md`:

```lua
-- ~/.config/nvim/after/ftplugin/rex.lua (or in your init.lua)
vim.lsp.start({
  name = 'rex',
  cmd = { 'rex', 'lsp' },
  root_dir = vim.fs.dirname(
    vim.fs.find({ '*.rexd', '.git' }, { upward = true })[1]
  ),
  filetypes = { 'rex', 'rexd' },
})
```

For `nvim-lspconfig` users, a custom server definition is used until Rex is added to the official registry:

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')
if not configs.rex then
  configs.rex = {
    default_config = {
      cmd = { 'rex', 'lsp' },
      filetypes = { 'rex', 'rexd' },
      root_dir = lspconfig.util.root_pattern('*.rexd', '.git'),
    },
  }
end
lspconfig.rex.setup({})
```

### OpenCode LSP configuration

OpenCode reads `opencode.json` in the workspace root. Add this:

```json
{
  "lsp": {
    "rex": {
      "command": ["rex", "lsp"],
      "filetypes": ["rex", "rexd"]
    }
  }
}
```

OpenCode will surface LSP diagnostics to the AI model so it can reason about Rex code health.

---

## Part 2: `rex mcp` — Model Context Protocol

MCP is a separate protocol from LSP. Where LSP provides **passive language intelligence** (diagnostics, completions, hover), MCP provides **active tool-calling** — the AI agent explicitly calls tools with arguments and gets back results.

### Why both?

- LSP is what makes **OpenCode and editors** aware of errors as you type
- MCP is what lets **AI agents invoke Rex tools** from a conversation ("compile this snippet", "check this file for type errors", "encode this expression to bytecode")

Both use JSON-RPC 2.0 over stdio — the wire format is similar but the message schema differs.

### Command definition (add to `crates/rex-cli/src/main.rs`)

```rust
/// Start the Model Context Protocol server over stdio
Mcp {
    /// Domain interface file (.rexd). Auto-discovered if not specified.
    #[arg(long)]
    domain: Option<PathBuf>,
},
```

### Dependencies to add to `crates/rex-cli/Cargo.toml`

MCP does not have a stable Rust crate yet. Implement the transport manually — it's a simple JSON-RPC 2.0 loop over stdin/stdout with a small set of MCP lifecycle messages. Use `serde_json` (already added for LSP).

Alternatively, if the `rmcp` or `mcp-server` crate is available on crates.io at the time of implementation, evaluate it. Otherwise, the manual approach is fine — MCP's message schema is small.

### Module layout

```
crates/rex-cli/src/
  mcp/
    mod.rs          # Server entry, stdio JSON-RPC loop, lifecycle (initialize/ping/shutdown)
    tools.rs        # Tool definitions and dispatch
    resources.rs    # (Optional) Resource listing for .rex and .rexd files
```

### MCP lifecycle

The MCP server must handle these JSON-RPC messages:

| Method | Description |
|---|---|
| `initialize` | Return server info, capabilities, and tool list |
| `notifications/initialized` | Client acknowledges — server is ready |
| `ping` | Respond with empty result |
| `tools/list` | Return the list of available tools with schemas |
| `tools/call` | Invoke a tool by name with arguments |
| `shutdown` | Clean up and exit |

### Tools to expose

| Tool name | Input | Output | Notes |
|---|---|---|---|
| `rex_check` | `{ "source": string, "domain"?: string }` | `{ "diagnostics": DiagnosticList }` | Run parse + typecheck on a source snippet |
| `rex_compile` | `{ "source": string, "domain"?: string }` | `{ "bytecode": string }` | Compile to REXC bytecode (base64 or hex encoded) |
| `rex_parse` | `{ "source": string }` | `{ "ast": object }` | Return the parse tree as JSON |
| `rex_format` | `{ "source": string }` | `{ "formatted": string }` | Auto-format Rex source (if formatting is implemented) |
| `rex_eval` | `{ "source": string, "input": object, "domain"?: string }` | `{ "output": object }` | Evaluate a Rex expression against an input (useful for testing) |

The `domain` parameter for each tool follows the same priority as the LSP:
1. Explicit `domain` argument in the tool call
2. `domain` from `initialize` `params.meta.domain`
3. Auto-discovery via `find_rexd` in the current working directory

### Wire format

MCP messages are newline-delimited JSON over stdin/stdout (same as LSP Content-Length framing is **not** required for MCP stdio — messages are newline-delimited). Verify this against the MCP spec at implementation time; the spec may have been updated.

**Example `initialize` exchange:**

```json
// Client → Server (stdin)
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"opencode","version":"1.0"}}}

// Server → Client (stdout)
{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"rex","version":"0.1.0"}}}
```

**Example `tools/call` exchange:**

```json
// Client → Server
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"rex_check","arguments":{"source":"x = req.method\nwhen x do\n  y = req.body\nend","domain":"./server.rexd"}}}

// Server → Client
{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"No errors found."}]}}
```

### How agents configure `rex mcp`

**OpenCode** (`opencode.json` in workspace root):

```json
{
  "mcp": {
    "rex": {
      "type": "local",
      "command": ["rex", "mcp"]
    }
  }
}
```

**Claude Desktop** (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "rex": {
      "command": "rex",
      "args": ["mcp"]
    }
  }
}
```

**Cursor** — add via Settings → MCP (same stdio pattern as above).

---

## Part 3: `rex-wasm` — Rust Compiled to WASM

This is a new crate that wraps `rex-core` and exposes its capabilities to the browser via `wasm-bindgen`. It powers the VS Code Web extension's Web Worker.

### Why a separate crate?

`rex-core` must remain usable as a native Rust library. Mixing `wasm-bindgen` attributes into it would complicate the build. A thin wrapper crate (`rex-wasm`) keeps the concerns separate.

### Add to `packages/rusty-rex/Cargo.toml`

```toml
[workspace]
members = ["crates/*"]
```

(The glob already covers it — just create `crates/rex-wasm/`.)

Add to workspace dependencies:

```toml
wasm-bindgen = "0.2"
```

### `crates/rex-wasm/Cargo.toml`

```toml
[package]
name = "rex-wasm"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
rex-core.workspace = true
wasm-bindgen = "0.2"
serde = { version = "1", features = ["derive"] }
serde-wasm-bindgen = "0.6"
```

> **Note:** Do NOT add `wasm-bindgen` to the workspace dependencies of `crates` that already compile as native binaries (rex-cli, rex-node). Only `rex-wasm` needs it.

### `crates/rex-wasm/src/lib.rs`

```rust
use wasm_bindgen::prelude::*;

/// Parse a Rex source string and return diagnostics as a JS value.
/// Returns an array of `{ message: string, start: number, end: number, severity: string }`.
#[wasm_bindgen]
pub fn check(source: &str, rexd_source: &str) -> Result<JsValue, JsValue> {
    let tokens = rex_core::lexer::lex(source);
    let (_, parse_errors) = rex_core::parser::parse(source, &tokens);
    // TODO: also run typecheck if rexd_source is non-empty
    let diagnostics: Vec<_> = parse_errors
        .iter()
        .map(|e| DiagnosticJs {
            message: e.message.clone(),
            start: e.span.start,
            end: e.span.end,
            severity: "error".to_string(),
        })
        .collect();
    serde_wasm_bindgen::to_value(&diagnostics).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Compile a Rex source string to REXC bytecode, returned as a Uint8Array.
#[wasm_bindgen]
pub fn compile(source: &str, rexd_source: &str) -> Result<Vec<u8>, JsValue> {
    // TODO: call rex_core encoding pipeline
    todo!()
}

#[derive(serde::Serialize)]
struct DiagnosticJs {
    message: String,
    start: usize,
    end: usize,
    severity: String,
}
```

Expand the API surface to match what the LSP Web Worker needs (see Part 4).

### Building the WASM

```sh
# Install wasm-pack once
cargo install wasm-pack

# Build for Web Worker (no-modules target = cross-browser safe)
wasm-pack build crates/rex-wasm --target no-modules --out-dir ../../packages/vscode-rex/wasm

# The output files in packages/vscode-rex/wasm/:
#   rex_wasm.js          # importScripts-loadable glue
#   rex_wasm_bg.wasm     # actual WASM binary
#   rex_wasm.d.ts        # TypeScript declarations
```

Add this as a build script in `packages/rusty-rex/`:

```json
// packages/rusty-rex/package.json (create if doesn't exist, or Makefile)
{
  "scripts": {
    "build:wasm": "wasm-pack build crates/rex-wasm --target no-modules --out-dir ../../packages/vscode-rex/wasm"
  }
}
```

Add the `wasm/` output to `packages/vscode-rex/.vscodeignore` exclusions carefully — the `.wasm` file MUST be included in the packaged extension. Check `.vscodeignore` excludes source maps but includes `wasm/`.

### WASM in the VS Code Web Worker

Inside the Web Worker (`server-browser.ts`), load the WASM like this:

```typescript
// In the Web Worker (service worker / dedicated worker context)
// importScripts is available in Web Worker but NOT in ES module workers
declare function importScripts(...urls: string[]): void;
declare const wasm_bindgen: any;

async function loadWasm(baseUri: string): Promise<void> {
    importScripts(`${baseUri}/wasm/rex_wasm.js`);
    await wasm_bindgen(`${baseUri}/wasm/rex_wasm_bg.wasm`);
}
```

The `baseUri` is the extension's `extensionUri`, passed from the extension host to the worker via the initial `postMessage`.

---

## Part 4: VS Code Extension Update

The extension needs **two entry points**:

| Entry | Field in `package.json` | Runtime | Transport |
|---|---|---|---|
| Desktop | `"main"` | Node.js extension host | Child process stdio (`vscode-languageclient/node`) |
| Web | `"browser"` | Browser Web Worker host | Web Worker postMessage (`vscode-languageclient/browser`) |

### 4a. Desktop entry (`src/extension.ts`)

Replace all manual providers (diagnostics, completions, hover, symbols, definitions, references, semantic tokens) with a single `LanguageClient` that spawns `rex lsp`:

```typescript
import { LanguageClient, TransportKind } from 'vscode-languageclient/node';
import * as vscode from 'vscode';

export function activate(context: vscode.ExtensionContext) {
    // Find the rex binary
    const rexPath = findRexBinary(context);  // see below
    if (!rexPath) {
        vscode.window.showWarningMessage('Rex CLI not found. Install rex and ensure it is on PATH.');
        // Still register RxViewerProvider
        context.subscriptions.push(RxViewerProvider.register(context));
        return;
    }

    const client = new LanguageClient(
        'rex',
        'Rex Language Server',
        {
            run: { command: rexPath, args: ['lsp'], transport: TransportKind.stdio },
            debug: { command: rexPath, args: ['lsp'], transport: TransportKind.stdio },
        },
        {
            documentSelector: [
                { scheme: 'file', language: 'rex' },
                { scheme: 'file', language: 'rexd' },
            ],
            initializationOptions: {
                // Pass domain file from VS Code settings if configured
                domain: vscode.workspace.getConfiguration('rex').get<string>('domainFile'),
            },
        },
    );

    context.subscriptions.push(client);
    client.start();

    // Keep the RX/REXC custom viewer
    context.subscriptions.push(RxViewerProvider.register(context));
}

function findRexBinary(context: vscode.ExtensionContext): string | undefined {
    // 1. Check PATH
    const { execSync } = require('child_process');
    try { execSync('rex --version'); return 'rex'; } catch {}

    // 2. Check workspace's rusty-rex target (dev mode)
    const ws = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (ws) {
        const devPath = require('path').join(ws, 'packages/rusty-rex/target/debug/rex');
        if (require('fs').existsSync(devPath)) return devPath;
        const releasePath = require('path').join(ws, 'packages/rusty-rex/target/release/rex');
        if (require('fs').existsSync(releasePath)) return releasePath;
    }

    return undefined;
}
```

### 4b. Web Worker server (`src/server-browser.ts`)

This file runs inside a Web Worker. It implements the same LSP handler logic as the native server but uses `rex-wasm` instead of spawning a process.

```typescript
import {
    createConnection,
    TextDocuments,
    Diagnostic,
    DiagnosticSeverity,
    BrowserMessageReader,
    BrowserMessageWriter,
    TextDocumentSyncKind,
    InitializeResult,
} from 'vscode-languageserver/browser';
import { TextDocument } from 'vscode-languageserver-textdocument';

// WASM is loaded lazily; the extension host sends the base URI via initializationOptions
let wasmReady = false;

const messageReader = new BrowserMessageReader(self);
const messageWriter = new BrowserMessageWriter(self);
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
            textDocumentSync: TextDocumentSyncKind.Incremental,
            completionProvider: { triggerCharacters: ['.'] },
            hoverProvider: true,
            definitionProvider: true,
            diagnosticProvider: {
                interFileDependencies: false,
                workspaceDiagnostics: false,
            },
        },
    };
});

documents.onDidChangeContent(async (change) => {
    if (!wasmReady) return;
    const diagnostics = computeDiagnostics(change.document);
    connection.sendDiagnostics({ uri: change.document.uri, diagnostics });
});

function computeDiagnostics(document: TextDocument): Diagnostic[] {
    const source = document.getText();
    // wasm_bindgen is loaded globally via importScripts in loadWasm
    const raw = (globalThis as any).wasm_bindgen?.check(source, '') ?? [];
    return raw.map((d: any) => ({
        range: {
            start: document.positionAt(d.start),
            end: document.positionAt(d.end),
        },
        message: d.message,
        severity: d.severity === 'error' ? DiagnosticSeverity.Error : DiagnosticSeverity.Warning,
        source: 'rex',
    }));
}

async function loadWasm(baseUri: string): Promise<void> {
    // importScripts is available in dedicated Web Workers
    (globalThis as any).importScripts(`${baseUri}/wasm/rex_wasm.js`);
    await (globalThis as any).wasm_bindgen(`${baseUri}/wasm/rex_wasm_bg.wasm`);
}

documents.listen(connection);
connection.listen();
```

### 4c. Browser extension entry (`src/browser.ts`)

This file is the `"browser"` entrypoint. It runs in the web extension host (not a worker), creates the Worker, and connects a LanguageClient to it.

```typescript
import { LanguageClient } from 'vscode-languageclient/browser';
import * as vscode from 'vscode';
import { RxViewerProvider } from './rx-viewer';

export function activate(context: vscode.ExtensionContext) {
    const serverModule = vscode.Uri.joinPath(context.extensionUri, 'dist/server-browser.js');

    const worker = new Worker(serverModule.toString(true));

    const client = new LanguageClient(
        'rex',
        'Rex Language Server (Web)',
        { documentSelector: [{ language: 'rex' }, { language: 'rexd' }] },
        worker,
    );

    // Pass extensionUri to the worker so it can load WASM
    // The worker receives this in params.initializationOptions.extensionUri
    // This requires custom initializationOptions support in LanguageClient — see below.

    context.subscriptions.push(client);
    client.start();

    // Note: RxViewerProvider uses Node.js APIs and is NOT available in the web extension.
    // It must be conditionally registered only in the desktop entry.
}
```

> **Important:** To pass `extensionUri` to the worker, use `clientOptions.initializationOptions`:
> ```typescript
> initializationOptions: { extensionUri: context.extensionUri.toString() }
> ```

### 4d. `package.json` updates

```json
{
  "main": "./dist/extension.js",
  "browser": "./dist/browser.js",
  "scripts": {
    "build:wasm": "cd ../rusty-rex && wasm-pack build crates/rex-wasm --target no-modules --out-dir ../../packages/vscode-rex/wasm",
    "build:webview": "cd ../web-viewer && bun vite build --config vite.webview.config.ts",
    "build:extension": "bun build src/extension.ts --outfile dist/extension.js --external vscode --format cjs --target node",
    "build:browser": "bun build src/browser.ts --outfile dist/browser.js --external vscode --format esm --target browser",
    "build:server-browser": "bun build src/server-browser.ts --outfile dist/server-browser.js --format esm --target browser",
    "build": "bun run build:wasm && bun run build:webview && bun run build:extension && bun run build:browser && bun run build:server-browser"
  },
  "devDependencies": {
    "@types/vscode": "^1.75.0",
    "vscode-languageclient": "^9.0.0",
    "vscode-languageserver": "^9.0.0",
    "vscode-languageserver-textdocument": "^1.0.0"
  }
}
```

> **Note on bundling `server-browser.ts`:** Bun's `--target browser` does not support Web Worker globals out of the box. You may need to configure `--define 'self=globalThis'` or use esbuild directly with `platform: 'browser'` and `format: 'iife'` for the worker bundle. Verify the output works in a Worker context before finalizing.

### 4e. What to remove from the extension

| File | Replace with |
|---|---|
| `rex-diagnostics.ts` | LSP diagnostics (native) / `wasm_bindgen.check()` (web) |
| `rex-domain.ts` | LSP completions + hover |
| `rex-symbols.ts` | LSP document symbols + definitions + references |
| All manual VS Code provider classes in `extension.ts` | `LanguageClient` |

### 4f. What to keep

- `rx-viewer.ts` — custom RX/REXC editor (desktop only; not available in web extension)
- TextMate grammars — syntax highlighting remains client-side in both environments

### 4g. TextMate grammar updates

The current `.tmLanguage` files need updates for current Rex syntax:

**Keywords to add:**
`return`, `break`, `continue`, `delete`, `type`, `extern`, `mut`

**Keywords to remove:**
`self`, `nor`

**Template literals:**
- Backtick-delimited strings `` `...` ``
- `${expr}` interpolation inside backticks
- Tagged templates: identifier immediately before backtick
- Nested template literals inside `${...}`

**Type annotations:**
- `: Type` after variable names
- `-> Type` return type annotations
- `type Name = ...` declarations
- `extern name = Type` declarations
- `extern mut name = Type`
- `extern name.method(args) -> Type`

**String types:** double-quoted, single-quoted, backtick-template

**Comments:** `// line`, `/* block */`

**Numbers:** decimal, `0xFF` hex, `0b1010` binary

### 4h. File associations in `package.json`

Register `.rexd` as a language:

```json
{
  "id": "rexd",
  "aliases": ["Rex Domain", "rexd"],
  "extensions": [".rexd"],
  "configuration": "./language-configuration.json",
  "icon": {
    "light": "./img/rex-icon-light.svg",
    "dark": "./img/rex-icon-dark.svg"
  }
}
```

Update `documentSelector` in all LSP client configurations to include `{ language: "rexd" }`.

---

## Part 5: VS Code Settings

Add configuration contribution to `package.json`:

```json
"configuration": {
  "title": "Rex",
  "properties": {
    "rex.domainFile": {
      "type": "string",
      "description": "Path to the .rexd domain interface file. Auto-discovered from workspace if not set.",
      "default": ""
    },
    "rex.path": {
      "type": "string",
      "description": "Path to the rex binary. Uses PATH if not set.",
      "default": ""
    }
  }
}
```

---

## Implementation Order

### Phase 1: `rex lsp` — minimal diagnostics (native binary)

1. Add `lsp-server`, `lsp-types`, `serde_json`, `serde` to `crates/rex-cli/Cargo.toml`
2. Add `Lsp` variant to `Command` enum in `main.rs`
3. Create `src/lsp/mod.rs` — stdio transport, LSP handshake, initialize/shutdown
4. Create `src/lsp/document.rs` — `HashMap<Url, String>` document store
5. Create `src/lsp/diagnostics.rs` — parse + typecheck, publish on open/change/save
6. Verify: `echo '<initialize JSON>' | rex lsp` responds correctly

### Phase 2: `rex lsp` — completions, hover, go-to-definition

1. Create `src/lsp/completion.rs` — completions from `DomainSchema`
2. Create `src/lsp/hover.rs` — type lookup + doc comments
3. Create `src/lsp/definition.rs` — jump to `.rexd` source locations
4. Verify with a generic LSP test client or Neovim

### Phase 3: VS Code desktop extension

1. Add `vscode-languageclient` npm dependency
2. Rewrite `extension.ts` to use `LanguageClient` spawning `rex lsp`
3. Remove `rex-diagnostics.ts`, `rex-domain.ts`, `rex-symbols.ts`
4. Keep `rx-viewer.ts`
5. Update domain file discovery from `.config.rex` → `.rexd`
6. Add VS Code settings for `rex.domainFile` and `rex.path`
7. Verify: open a `.rex` file, confirm diagnostics come from Rust, completions work

### Phase 4: `rex-wasm` crate

1. Create `crates/rex-wasm/` with `Cargo.toml` and `src/lib.rs`
2. Expose `check(source, rexd_source) -> JsValue` at minimum
3. Build with `wasm-pack build --target no-modules`
4. Write a minimal HTML test harness to verify the WASM loads in a Worker
5. Add build script to `packages/rusty-rex`

### Phase 5: VS Code Web extension

1. Add `vscode-languageserver` and `vscode-languageserver-textdocument` npm deps
2. Create `src/server-browser.ts` (Web Worker LSP server using rex-wasm)
3. Create `src/browser.ts` (browser extension entry using `vscode-languageclient/browser`)
4. Add `"browser": "./dist/browser.js"` to `package.json`
5. Wire up build scripts for browser + server-browser bundles
6. Verify in [vscode.dev](https://vscode.dev): open a `.rex` file, confirm diagnostics appear (served from the Worker's WASM)

### Phase 6: `rex mcp`

1. Add `Mcp` variant to `Command` enum in `main.rs`
2. Create `src/mcp/mod.rs` — JSON-RPC loop over stdin/stdout, lifecycle messages
3. Create `src/mcp/tools.rs` — `rex_check`, `rex_compile`, `rex_parse` tools
4. Verify with OpenCode or Claude Desktop MCP config
5. Document configuration in README

### Phase 7: TextMate grammar refresh (independent)

1. Rewrite `syntaxes/rex.tmLanguage.json` for current Rex syntax
2. Add template literal patterns, `.rexd` highlighting
3. Add `.rexd` language registration to `package.json`
4. Run `bun test` in `packages/vscode-rex` to verify grammar tests pass

---

## Key Files to Reference

| File | What it provides |
|---|---|
| `crates/rex-core/src/typecheck.rs` | Type checker — `check_source(source, &schema)` returns diagnostics with spans |
| `crates/rex-core/src/parser.rs` | Parser — `parse(source, tokens)` returns green tree + errors |
| `crates/rex-core/src/lexer.rs` | Lexer — `lex(source)` returns tokens with spans |
| `crates/rex-core/src/syntax.rs` | CST node types and `SyntaxKind` enum |
| `crates/rex-cli/src/main.rs` | CLI entry — `find_rexd`, `offset_to_line_col`, `Command` enum to extend |
| `rex-types.md` | Type system specification and `.rexd` syntax |
| `language.md` | Full language reference |
| `rexc-bytecode.md` | Bytecode format (for `.rexc` highlighting and `rex_compile` tool) |
| `packages/vscode-rex/` | Existing VS Code extension to update |
| `packages/rusty-rex/examples/knowledge-base/rex-serve.rexd` | Real-world `.rexd` file for testing |

---

## Verification Checklist

```sh
# 1. LSP binary builds
cargo build -p rex-cli

# 2. LSP responds to initialize
printf 'Content-Length: 97\r\n\r\n{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{},"rootUri":null,"processId":null}}' | rex lsp

# 3. MCP responds to initialize
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}' | rex mcp

# 4. WASM builds
wasm-pack build crates/rex-wasm --target no-modules --out-dir ../../packages/vscode-rex/wasm

# 5. VS Code desktop: open example knowledge-base
cd packages/vscode-rex && code --extensionDevelopmentPath=. ../../packages/rusty-rex/examples/knowledge-base/routes/
# Open a .rex file → diagnostics from Rust parser/typechecker
# Type `json.` → completions from .rexd
# Hover over `headers` → type shown from .rexd
# Go-to-definition on an extern → jumps to .rexd

# 6. VS Code Web: test in web extension host
# In VS Code desktop: F1 → "Open Extension in Web Extension Host" (Ctrl+Shift+P → workbench.action.openExtensionHostedWindow)
# Or use: npx @vscode/test-web --extensionDevelopmentPath=. --browser chromium
# Open a .rex file → diagnostics via WASM in Web Worker

# 7. Neovim: open a .rex file with rex lsp configured
# Should see diagnostics, completions, hover, go-to-definition

# 8. OpenCode: add mcp config to opencode.json, start a session
# Agent should be able to call rex_check and get structured diagnostics

# 9. Grammar tests
cd packages/vscode-rex && bun test
# Template literals highlight correctly
# `return`, `type`, `extern`, `mut` are keyword-colored
# `self`, `nor` are NOT keyword-colored
```

## Known Constraints and Gotchas

### WASM build

- `wasm-pack --target no-modules` is required for cross-browser Web Worker compatibility. Firefox does not support ES module workers. Chrome-based VS Code environments would support `--target web`, but `no-modules` is safer.
- The output `.wasm` file must be included in the packaged `.vsix`. Check `packages/vscode-rex/.vscodeignore` — ensure `wasm/**` is NOT excluded.
- `logos` and `rowan` (used by `rex-core`) are pure Rust and compile to `wasm32-unknown-unknown` without modification — no `cfg` guards needed.
- Do NOT add `wasm-bindgen` to workspace dependencies shared with native crates. Keep it scoped to `rex-wasm/Cargo.toml`.

### Web Worker entry point bundling

- The `server-browser.ts` Worker script must be bundled as a self-contained file (no dynamic `require()`). Bun's `--format esm --target browser` should work but verify. The `importScripts()` call for WASM loading must run in the Worker context, not the main extension host.
- `rx-viewer.ts` uses `vscode.WebviewPanel` and Node.js APIs — do NOT include it in the browser entrypoint.

### LSP Content-Length framing

- LSP uses `Content-Length: N\r\n\r\n{...}` framing (HTTP-style headers), not newline-delimited JSON. The `lsp-server` crate handles this. MCP stdio uses newline-delimited JSON (no headers). Do not confuse the two.

### MCP protocol version

- The MCP spec evolves quickly. At implementation time, check [https://spec.modelcontextprotocol.io](https://spec.modelcontextprotocol.io) for the current stable version and adjust the `protocolVersion` string accordingly.

### `.config.rex` → `.rexd` migration

- The existing VS Code extension searches for `.config.rex`. The new LSP-based extension searches for `*.rexd`. Users with old `.config.rex` files should rename them to `server.rexd` (or whatever name fits). Document this migration.
