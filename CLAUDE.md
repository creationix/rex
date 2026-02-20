---
description: Rex repo tooling and command conventions.
globs: "*.ts, *.js, package.json"
alwaysApply: false
---

Default to using Bun commands in this repo.

- Use `bun run <script>` for package/workspace scripts.
- Use `bun test` for tests.

## Compiler Helper

Use the dedicated compiler instead of mentally deriving compact encodings:

```sh
bun run rex:compile -c --expr "when x do y end"
bun run rex:compile -c --file input.rex
bun run rex:compile --expr "a and b" --ir
```

Installable CLI alternative:

```sh
rex -c --expr "when x do y end"
rex --expr "a and b" --ir
```

## Common Commands

From repo root:

```sh
bun run rex:compile -c --expr "when x do y end"
bun run rex:verify-docs
```

From `packages/rex-lang`:

```sh
bun run build:grammar
bun test
```

From `packages/vscode-rex`:

```sh
bun test
bun run build
bun run reinstall
```

## Change Checklist

- After editing `packages/rex-lang/rex.ohm`, run `bun run build:grammar` in `packages/rex-lang`.
- After parser/IR/encoding changes, run `bun test` in `packages/rex-lang`.
- After docs examples change, run `bun run rex:verify-docs` from repo root.
- After VS Code extension tokenizer/grammar changes, run `bun test` and `bun run build` in `packages/vscode-rex`.

## Documentation

- [language.md](language.md) — Rex syntax and semantics reference
- [rexc-bytecode.md](rexc-bytecode.md) — `rexc` encoding specification
- [CONTRIBUTING.md](CONTRIBUTING.md) — repo layout, architecture, development workflow
