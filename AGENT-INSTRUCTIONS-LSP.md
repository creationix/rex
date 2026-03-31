# Instructions: Build a Rust LSP for Rex and Update the VS Code Extension

## Goal

Build a Rex Language Server Protocol (LSP) implementation in Rust and update the VS Code extension (`packages/vscode-rex`) to use it. The LSP should provide diagnostics, completions, hover, and go-to-definition powered by the existing `rex-core` type checker and parser.

## Context

- The canonical Rex compiler, parser, and type checker are in **Rust** at `packages/rusty-rex/crates/rex-core`
- The VS Code extension at `packages/vscode-rex` uses a **legacy TypeScript parser** that doesn't support template literals, `return`, variadic `and`/`or`, `type`/`extern` declarations, or the current bytecode format
- The type checker (`rex-core/src/typecheck.rs`) already validates Rex programs against `.rexd` domain interface files
- The CLI already has a `check` command (`rex check routes/ --domain server.rexd`) that runs the type checker
- Domain interface files (`.rexd`) declare types, extern bindings, and function signatures — see `rex-types.md` for the full spec

## Architecture

### New crate: `crates/rex-lsp/`

A standalone binary that speaks LSP over stdio. The VS Code extension spawns it as a child process.

```
crates/rex-lsp/
  Cargo.toml
  src/
    main.rs           LSP server entry, stdio transport
    server.rs         Request/notification dispatch
    diagnostics.rs    Parse errors + type checker diagnostics
    completion.rs     Completions from .rexd declarations
    hover.rs          Hover info from .rexd + inferred types
    document.rs       Document state management (open files, incremental updates)
```

### Dependencies

- `rex-core` (workspace) — lexer, parser, type checker
- `lsp-server` — LSP protocol implementation (from rust-analyzer)
- `lsp-types` — LSP type definitions
- `serde_json` — JSON serialization

### VS Code extension update

The extension needs:
1. A language client that spawns `rex-lsp` and communicates over stdio
2. Updated TextMate grammars for `.rex`, `.rexc`, and `.rexd` syntax highlighting
3. Remove the legacy TypeScript parser-based diagnostics

## Part 1: The LSP Server

### Capabilities to implement

**Must have:**
- `textDocument/publishDiagnostics` — parse errors + type checker warnings/errors on open and save
- `textDocument/completion` — complete extern names, type names, and object keys from `.rexd`
- `textDocument/hover` — show type and doc comment for identifiers
- `textDocument/definition` — jump to extern declaration in `.rexd`

**Nice to have:**
- `textDocument/documentSymbol` — outline view (variable assignments, when/for blocks)
- `textDocument/formatting` — auto-format Rex source
- `textDocument/signatureHelp` — show function parameter info for extern functions

### Document management

The LSP manages open documents in memory:

1. On `textDocument/didOpen` — parse the file, find the nearest `.rexd`, run type checker, publish diagnostics
2. On `textDocument/didChange` — re-parse and re-check the changed file
3. On `textDocument/didSave` — full re-check (in case .rexd changed)

### Finding the domain file

The LSP searches upward from the open file for `*.rexd` files, same as the type checker. The first `.rexd` found becomes the domain interface for all files in that directory tree.

### Diagnostics

Use `rex_core::typecheck::check(source, domain_source)` to get diagnostics. Each diagnostic has a span (byte range), message, and severity. Map these to LSP `Diagnostic` objects with line/column positions.

Also run `rex_core::parser::parse()` and report any parse errors as diagnostics.

### Completions

When the user types inside a Rex file, provide completions from:

1. **Extern bindings** — `req`, `res`, `headers`, `method`, `body`, `params`, etc.
2. **Extern functions** — `json.parse`, `db.get`, `time.uuid`, etc. (after typing `json.`)
3. **Type aliases** — `Headers`, `HttpMethod`, etc. (in type annotation positions)
4. **Object keys** — when typing inside `{`, suggest keys from the expected type
5. **Keywords** — `when`, `unless`, `for`, `while`, `do`, `end`, `else`, `return`, `and`, `or`, `in`, `of`, `true`, `false`, `null`, `none`

Extract these from the `.rexd` file by parsing it with `rex_core::parser::parse()` and walking the CST for `extern` and `type` declarations.

### Hover

When the user hovers over an identifier:

1. Look up the identifier in the `.rexd` declarations
2. If found, show the type and any doc comment (the `//` comment above the declaration)
3. If it's a local variable, show the inferred type from the type checker

### Go to definition

For extern bindings and functions, jump to their declaration in the `.rexd` file. This requires tracking the source location of each declaration in the `.rexd` file.

## Part 2: VS Code Extension Update

### Language client

Replace the legacy TypeScript parser with an LSP client:

```ts
import { LanguageClient, TransportKind } from 'vscode-languageclient/node';

const serverPath = /* path to rex-lsp binary */;
const client = new LanguageClient('rex', 'Rex Language Server', {
  run: { command: serverPath, transport: TransportKind.stdio },
  debug: { command: serverPath, transport: TransportKind.stdio },
}, {
  documentSelector: [
    { scheme: 'file', language: 'rex' },
    { scheme: 'file', language: 'rexd' },
  ],
});
```

The extension should:
- Bundle the `rex-lsp` binary (or expect it on PATH)
- Auto-discover it from the workspace's `packages/rusty-rex/target/` during development
- Show a "Rex LSP not found" warning if the binary is missing

### TextMate grammar updates

The current `.tmLanguage` files are very out of date. They need to support:

**Keywords (current Rex):**
`when`, `unless`, `do`, `end`, `else`, `for`, `in`, `of`, `while`, `and`, `or`, `not`, `return`, `break`, `continue`, `delete`, `true`, `false`, `null`, `none`, `type`, `extern`, `mut`

**Removed keywords:**
`self`, `nor` — remove these from the grammar

**Template literals:**
- Backtick-delimited strings with `${expr}` interpolation
- Tagged templates: `identifier` immediately before backtick
- Nested template literals inside `${...}`

**Type annotations:**
- `: Type` after variable names in assignments
- `-> Type` for function return types
- `type Name = ...` declarations
- `extern name = Type` declarations
- `extern mut name = Type`
- `extern name.method(args) -> Type`

**String types:**
- Double-quoted: `"..."`
- Single-quoted: `'...'`
- Template: `` `...${expr}...` ``

**Comments:**
- Line: `// ...`
- Block: `/* ... */`

**Numbers:**
- Decimal: `42`, `3.14`, `1e10`
- Hex: `0xFF`
- Binary: `0b1010`

### File associations

Register these file types:
- `.rex` — Rex source files
- `.rexd` — Rex domain interface files (same syntax, different semantics)
- `.rexc` — REXC bytecode (basic highlighting for the printable UTF-8 format)

## Implementation Order

### Phase 1: Minimal LSP with diagnostics
1. Create `crates/rex-lsp/` with Cargo.toml
2. Implement stdio transport and basic LSP handshake
3. Parse open documents and publish diagnostics (parse errors only)
4. Test with VS Code using a minimal extension client

### Phase 2: Type checker integration
1. Find `.rexd` files by searching upward from open documents
2. Run `typecheck::check()` and map diagnostics to LSP format
3. Re-check on save

### Phase 3: Completions and hover
1. Extract declarations from `.rexd` files
2. Provide completion items for externs, types, and keywords
3. Show hover info with types and doc comments

### Phase 4: VS Code extension overhaul
1. Replace TS parser with LSP client
2. Rewrite TextMate grammars for current Rex syntax
3. Add template literal and `.rexd` highlighting
4. Package and test

## Key Files to Reference

| File | What it provides |
|---|---|
| `crates/rex-core/src/typecheck.rs` | Type checker — `check(source, domain)` returns diagnostics |
| `crates/rex-core/src/parser.rs` | Parser — `parse(source, tokens)` returns CST + errors |
| `crates/rex-core/src/lexer.rs` | Lexer — `lex(source)` returns tokens with spans |
| `crates/rex-core/src/syntax.rs` | CST node types and `SyntaxKind` enum |
| `crates/rex-cli/src/main.rs` | CLI `check` command — reference for how to call the type checker |
| `rex-types.md` | Type system specification and `.rexd` syntax |
| `language.md` | Full language reference |
| `rexc-bytecode.md` | Bytecode format (for `.rexc` highlighting) |
| `packages/vscode-rex/` | Existing VS Code extension to update |
| `packages/rusty-rex/examples/knowledge-base/rex-serve.rexd` | Real-world `.rexd` file with all opcode declarations |

## Verification

```sh
# 1. LSP binary builds
cargo build -p rex-lsp

# 2. Open VS Code with the extension
cd packages/vscode-rex && code --extensionDevelopmentPath=. ../../packages/rusty-rex/examples/knowledge-base/routes/

# 3. Open a .rex file — should see diagnostics, completions, hover
# 4. Open the .rexd file — should have proper highlighting
# 5. Type `json.` — should see parse, stringify completions
# 6. Hover over `headers` — should show type from .rexd
# 7. Template literals should highlight: backticks, ${}, expressions inside
# 8. `return`, `type`, `extern`, `mut` should be keyword-colored
```
