# Instructions: Add LSP Support to the Rex CLI

## Goal

Add a `rex lsp` subcommand that speaks the Language Server Protocol over stdio. This gives any LSP-capable editor or headless agent (open-code, Cursor, Zed, Neovim, etc.) diagnostics, completions, hover, and go-to-definition powered by the existing `rex-core` parser and type checker. Then update the VS Code extension to use it.

## Context

- The canonical Rex compiler, parser, and type checker are in **Rust** at `packages/rusty-rex/crates/rex-core`
- The CLI at `crates/rex-cli` already has a `check` subcommand that runs the type checker with `.rexd` auto-discovery
- The VS Code extension at `packages/vscode-rex` uses a **legacy TypeScript parser** that doesn't support template literals, `return`, variadic `and`/`or`, `type`/`extern` declarations, or the current bytecode format
- The type checker (`rex-core/src/typecheck.rs`) already validates Rex programs against `.rexd` domain interface files
- Domain interface files (`.rexd`) declare types, extern bindings, and function signatures — see `rex-types.md` for the full spec

## Architecture

### `rex lsp` subcommand in `crates/rex-cli/`

The LSP lives inside `rex-cli` as a module — no separate crate. It reuses the existing `find_rexd`, `collect_rex_files`, and `offset_to_line_col` helpers already in `main.rs`.

```
crates/rex-cli/
  Cargo.toml              Add lsp-server, lsp-types, serde_json deps
  src/
    main.rs               Add Lsp variant to Command enum
    lsp/
      mod.rs              LSP server entry, stdio transport, request dispatch
      diagnostics.rs      Parse errors + type checker diagnostics
      completion.rs       Completions from .rexd declarations
      hover.rs            Hover info from .rexd + inferred types
      definition.rs       Go-to-definition in .rexd files
      document.rs         Document state management (open files, incremental updates)
```

### Dependencies to add to `crates/rex-cli/Cargo.toml`

```toml
lsp-server = "0.7"
lsp-types = "0.97"
serde_json = "1"
```

### How agents use it

```sh
# With explicit domain file
rex lsp --domain server.rexd

# With auto-discovery (searches upward for *.rexd)
rex lsp
```

The binary speaks LSP JSON-RPC over stdin/stdout. Any LSP client connects the same way — VS Code, Neovim, open-code, etc.

## Part 1: The LSP Server

### Command definition

Add to the `Command` enum in `main.rs`:

```rust
/// Start the Language Server Protocol server
Lsp {
    /// Domain interface file (.rexd). Auto-discovered if not specified.
    #[arg(long)]
    domain: Option<PathBuf>,
},
```

### Capabilities to implement

**Must have:**
- `textDocument/publishDiagnostics` — parse errors + type checker warnings/errors on open and save
- `textDocument/diagnostic` (pull model) — agents request diagnostics on demand
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
3. On `textDocument/didSave` — full re-check (in case `.rexd` changed)

### Finding the domain file

Domain discovery uses the same `find_rexd` logic already in `main.rs` — search upward from the open file for `*.rexd` files. The first `.rexd` found becomes the domain interface for all files in that directory tree.

Priority order:
1. `--domain` CLI flag (if provided)
2. `initializationOptions.domain` from the LSP `initialize` request
3. Auto-discovery via `find_rexd` searching upward from the open file

### Diagnostics

Use `rex_core::typecheck::check_source(source, &schema)` to get diagnostics. Each `Diagnostic` has a `span` (byte range), `message`, and `kind` (Error/Warning). Map these to LSP `Diagnostic` objects using `offset_to_line_col` for line/column positions.

Also run `rex_core::parser::parse()` and report any parse errors as diagnostics.

If the `.rexd` file itself has parse errors, report those too (so the user knows why type info is missing).

### Completions

When the user types inside a Rex file, provide completions from:

1. **Extern bindings** — `req`, `res`, `headers`, `method`, `body`, `params`, etc.
2. **Extern functions** — `json.parse`, `db.get`, `time.uuid`, etc. (after typing `json.`)
3. **Type aliases** — `Headers`, `HttpMethod`, etc. (in type annotation positions)
4. **Object keys** — when typing inside `{`, suggest keys from the expected type
5. **Keywords** — `when`, `unless`, `for`, `while`, `do`, `end`, `else`, `return`, `and`, `or`, `in`, `of`, `true`, `false`, `null`, `none`

Extract these from the `DomainSchema` which already has `globals`, `functions`, and `type_aliases` fields parsed from the `.rexd` file.

### Hover

When the user hovers over an identifier:

1. Look up the identifier in the `DomainSchema` globals and functions
2. If found, show the type and any doc comment (the `//` comment above the declaration)
3. If it's a local variable, show the inferred type from the type checker

### Go to definition

For extern bindings and functions, jump to their declaration in the `.rexd` file. This requires tracking the source location of each declaration in the `.rexd` file.

## Part 2: VS Code Extension Update

### Language client

Replace the legacy TypeScript parser with an LSP client:

```ts
import { LanguageClient, TransportKind } from 'vscode-languageclient/node';

const serverPath = /* path to rex binary */;
const client = new LanguageClient('rex', 'Rex Language Server', {
  run: { command: serverPath, args: ['lsp'], transport: TransportKind.stdio },
  debug: { command: serverPath, args: ['lsp'], transport: TransportKind.stdio },
}, {
  documentSelector: [
    { scheme: 'file', language: 'rex' },
    { scheme: 'file', language: 'rexd' },
  ],
});
```

The extension should:
- Look for `rex` on PATH
- Auto-discover from the workspace's `packages/rusty-rex/target/` during development
- Show a "Rex CLI not found" warning if the binary is missing
- Pass the domain file via `initializationOptions` if configured in VS Code settings

### What to remove from the extension

- `rex-diagnostics.ts` — the legacy Ohm grammar parser; replaced by LSP diagnostics
- `rex-domain.ts` — domain schema parsing in TS; replaced by LSP completions/hover
- `rex-symbols.ts` — symbol analysis in TS; replaced by LSP document symbols / definition / references
- All the manual VS Code provider classes (semantic tokens, completions, hover, definitions, references) — the LSP client handles these automatically

### What to keep

- `rx-viewer.ts` — the RX/REXC custom editor (this is a VS Code-specific feature, not an LSP concern)
- TextMate grammars (syntax highlighting is still client-side)

### TextMate grammar updates

The current `.tmLanguage` files need to support:

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
- `.rexd` — Rex domain interface files (same grammar, shared with `.rex`)
- `.rexc` — REXC bytecode (basic highlighting for the printable UTF-8 format)

## Implementation Order

### Phase 1: Minimal `rex lsp` with diagnostics
1. Add `lsp-server`, `lsp-types`, `serde_json` to `crates/rex-cli/Cargo.toml`
2. Add `Lsp` variant to `Command` enum in `main.rs`
3. Create `src/lsp/mod.rs` — stdio transport, LSP handshake, initialize/shutdown
4. Create `src/lsp/document.rs` — track open documents in a `HashMap<Url, String>`
5. Create `src/lsp/diagnostics.rs` — parse + type-check, publish diagnostics on open/change/save
6. Test by piping LSP JSON over stdin or with a minimal client script

### Phase 2: Completions, hover, go-to-definition
1. Create `src/lsp/completion.rs` — extract completions from `DomainSchema`
2. Create `src/lsp/hover.rs` — look up types and doc comments
3. Create `src/lsp/definition.rs` — jump to `.rexd` source locations
4. Test with a generic LSP client

### Phase 3: VS Code extension wiring
1. Add `vscode-languageclient` dependency to `packages/vscode-rex`
2. Replace all manual providers with `LanguageClient` that spawns `rex lsp`
3. Remove `rex-diagnostics.ts`, `rex-domain.ts`, `rex-symbols.ts`
4. Keep `rx-viewer.ts`
5. Migrate domain file discovery from `.config.rex` to `.rexd`

### Phase 4: TextMate grammar refresh (independent track)
1. Rewrite TextMate grammars for current Rex syntax
2. Add template literal and `.rexd` highlighting
3. Test with `bun test` in `packages/vscode-rex`

## Key Files to Reference

| File | What it provides |
|---|---|
| `crates/rex-core/src/typecheck.rs` | Type checker — `check_source(source, &schema)` returns diagnostics with spans |
| `crates/rex-core/src/parser.rs` | Parser — `parse(source, tokens)` returns green tree + errors (always produces a tree) |
| `crates/rex-core/src/lexer.rs` | Lexer — `lex(source)` returns tokens with spans |
| `crates/rex-core/src/syntax.rs` | CST node types and `SyntaxKind` enum |
| `crates/rex-cli/src/main.rs` | CLI entry — `find_rexd`, `offset_to_line_col`, `Command` enum to extend |
| `rex-types.md` | Type system specification and `.rexd` syntax |
| `language.md` | Full language reference |
| `rexc-bytecode.md` | Bytecode format (for `.rexc` highlighting) |
| `packages/vscode-rex/` | Existing VS Code extension to update |
| `packages/rusty-rex/examples/knowledge-base/rex-serve.rexd` | Real-world `.rexd` file |

## Verification

```sh
# 1. CLI builds with LSP support
cargo build -p rex-cli

# 2. LSP starts and responds to initialize
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}' | rex lsp

# 3. Open VS Code with the updated extension
cd packages/vscode-rex && code --extensionDevelopmentPath=. ../../packages/rusty-rex/examples/knowledge-base/routes/

# 4. Open a .rex file — should see diagnostics from the Rust parser + type checker
# 5. Type `json.` — should see completions from .rexd
# 6. Hover over `headers` — should show type from .rexd
# 7. Go-to-definition on an extern — should jump to .rexd
# 8. Template literals should highlight: backticks, ${}, expressions inside
# 9. `return`, `type`, `extern`, `mut` should be keyword-colored
```
