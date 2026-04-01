# Contributing to Rex

## Repo Layout

```
crates/              — Rust compiler, LSP, interpreter, WASM, node bindings
packages/vscode-rex  — VS Code extension: syntax highlighting, LSP client
packages/rex-ts      — TypeScript tagged template utilities for Rex
examples/            — example programs (fibonacci, primes, gradebook, etc.)
```

## Prerequisites

- [Rust](https://rustup.rs) for the compiler
- [Bun](https://bun.sh) for the VS Code extension and TypeScript packages

## Common Commands

### Rust compiler

```sh
cargo build                              # build all crates
cargo test -p rex-core                   # run compiler tests
cargo install --path crates/rex-cli      # install rex CLI
```

### Rex CLI

```sh
rex compile --expr "when x do y end"     # compile to bytecode
rex check examples/case-studies/gradebook.rex  # type-check a file
rex run examples/fibonacci.rex                 # run a program
rex lsp                                  # start language server
```

### VS Code extension (`packages/vscode-rex`)

```sh
bun test                # run extension tests
bun run build           # build extension
bun run reinstall       # install extension locally
```

## Architecture

The compiler pipeline lives in `crates/rex-core`:

1. **Lexer** (`lexer.rs`) — tokenizes Rex source
2. **Parser** (`parser.rs`) — builds a concrete syntax tree (rowan)
3. **Lower** (`lower.rs`) — lowers CST to bytecode value tree
4. **Encode** (`bytecode.rs`) — serializes to compact REXC bytecode
5. **Typecheck** (`typecheck.rs`) — type inference and checking from `.rexd` schemas
6. **Decompile** (`decompile.rs`) — pretty-prints bytecode back to Rex source
7. **Interpret** (`interpret.rs`) — executes REXC bytecode

## Change Checklist

| What changed | What to run |
|---|---|
| Rust compiler (parser, lowerer, typechecker) | `cargo test -p rex-core` |
| VS Code grammar or extension | `bun test` and `bun run build` in `packages/vscode-rex` |
| After installing new rex binary | `cargo install --path crates/rex-cli` |

## Publishing

### VS Code extension

From `packages/vscode-rex`:

```sh
bun run package    # create .vsix
bun run publish    # publish to marketplace
```
