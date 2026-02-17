---
description: Rex repo tooling and command conventions.
globs: "*.ts, *.js, package.json"
alwaysApply: false
---

Default to using Bun commands in this repo.

- Use `bun run <script>` for package/workspace scripts.
- Use `bun test` for tests.

## Repo Tooling (Rex)

Use the dedicated compiler helper instead of mentally deriving compact encodings:

```sh
bun run rex:compile --expr "when x do y end"
bun run rex:compile --file input.rex
cat input.rex | bun run rex:compile
```

Use `--ir` when you want lowered IR JSON instead of compact encoding.

Use the doc verifier after grammar/encoding changes:

```sh
bun run rex:verify-docs
```

## Workspace Layout

- `packages/rex-lang`: Rex grammar, IR lowering, encoder/compiler, and docs verifier.
- `packages/vscode-rex`: VS Code extension (TextMate + semantic tokenization for `rex`/`rexc`).
- Root scripts proxy into `packages/rex-lang` for compile and doc verification.

## Common Commands

From repo root:

```sh
bun run rex:compile --expr "when x do y end"
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
