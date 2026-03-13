# Contributing to Rex

## Repo Layout

```
packages/rex-lang    — compiler: grammar, parser, IR, optimizer, encoder, CLI
packages/rx-format   — rx data tool: REXC cursor parser and CLI (@creationix/rx)
packages/vscode-rex  — VS Code extension: syntax highlighting, diagnostics, symbols
samples/             — example programs (fibonacci, primes, HTTP domain policies)
```

## Prerequisites

Rex uses [Bun](https://bun.sh) for all development tasks.

## Common Commands

From repo root:

```sh
bun run rex:compile --expr "when x do y end"
bun run rex:compile --file input.rex
bun run rex:compile --expr "a and b" --ir     # show lowered IR instead of bytecode
bun run rex:verify-docs                        # verify examples in language.md and rexc-bytecode.md
```

From `packages/rex-lang`:

```sh
bun test                # run compiler tests
bun run build:grammar   # regenerate Ohm grammar bundle after editing rex.ohm
```

From `packages/vscode-rex`:

```sh
bun test                # run extension tests
bun run build           # build extension
bun run reinstall       # install extension locally
```

## Installable CLI

```sh
bun add -g @creationix/rex
rex --help
rex --expr "when x do y end"
rex --file input.rex
```

Zero-install alternatives:

```sh
bunx @creationix/rex --expr "when x do y end"
npx -y @creationix/rex -- --expr "when x do y end"
```

## Data Tool (`rx`)

The `rx` CLI lives in `packages/rx-format` (published as `@creationix/rx`). See its [README](packages/rx-format/README.md) for full usage.

## Architecture

The compiler pipeline lives in `packages/rex-lang/rex.ts`:

1. **Parse** — `parseToIR()` uses the Ohm grammar (`rex.ohm`) to parse Rex source into an IR
2. **Optimize** — `optimizeIR()` inlines variables, eliminates dead code, deduplicates values
3. **Encode** — `encodeIR()` serializes IR to compact `rexc` bytecode
4. **Compile** — `compile()` wraps all three steps

The bytecode interpreter (`rexc-interpreter.ts`) executes `rexc` for tests and the sample harness.

## Change Checklist

| What changed | What to run |
|---|---|
| `rex.ohm` grammar | `bun run build:grammar` then `bun test` in `packages/rex-lang` |
| Parser, IR, encoder, or interpreter | `bun test` in `packages/rex-lang` |
| rx cursor parser or CLI | `bun test` in `packages/rx-format` |
| Doc examples (`language.md`, `rexc-bytecode.md`) | `bun run rex:verify-docs` from repo root |
| VS Code grammar or tokenizer | `bun test` and `bun run build` in `packages/vscode-rex` |

## Publishing

### npm package (`@creationix/rex`)

From `packages/rex-lang`:

```sh
npm whoami
bun run prepublishOnly
npm publish --access public
```

### VS Code extension

From `packages/vscode-rex`:

```sh
bun run package    # create .vsix
bun run publish    # publish to marketplace
```
