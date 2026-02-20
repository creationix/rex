# Copilot Instructions for Rex

## Tooling + command defaults
- Use Bun everywhere in this repo.
- Run workspace scripts from repo root: `bun run rex:compile`, `bun run rex:verify-docs`.
- Installable CLI is available as `rex` (via `bun add -g @creationix/rex`) and may be used instead of `bun run rex:compile`.
- No-install invocation is also supported via `bunx @creationix/rex ...` or `npx -y @creationix/rex -- ...`.
- Package-level scripts:
  - `packages/rex-lang`: `bun test`, `bun run build:grammar`
  - `packages/vscode-rex`: `bun test`, `bun run build`, `bun run reinstall`

## Big picture architecture
- `packages/rex-lang/rex.ts` is the core pipeline: parse (`parseToIR`) -> optional optimize (`optimizeIR`) -> encode (`encodeIR`) -> `compile`.
- `packages/rex-lang/rex.ohm` defines the language grammar; regenerate bundle after edits (`bun run build:grammar`).
- `packages/rex-lang/rexc-interpreter.ts` executes compact `rexc` for tests/samples (`evaluateSource`, `evaluateRexc`).
- Root scripts are thin proxies into `packages/rex-lang` (see `package.json`).

## Language and encoding conventions
- Primary source language is high-level infix Rex (`.rex`), not legacy s-expression syntax.
- Existence semantics are intentional (`undefined` = absent); avoid converting logic to JS-style truthiness.
- Keep encode/decode boundaries stable: assignment/place parsing in `rexc-interpreter.ts` depends on encoded value boundaries from `rex.ts`.
- Prefer validating parser/encoder behavior with `rex --expr "..." --ir` (or `bunx @creationix/rex --expr "..." --ir`, `npx -y @creationix/rex -- --expr "..." --ir`, or `bun run rex:compile --expr "..." --ir`) before changing tests.

## VS Code extension boundaries
- `packages/vscode-rex/src/extension.ts` wires diagnostics, symbols, definition/reference, completion/hover, and semantic tokens.
- TextMate grammars live in `packages/vscode-rex/syntaxes/*.tmLanguage.json` (rex, rexc, markdown fences, TS/JS template injections).
- Domain-aware completion/hover uses `.config.rex` at workspace root via `src/rex-domain.ts`.
- `.rexc` editor defaults (word wrap, separators) are in `packages/vscode-rex/package.json` under `contributes.configurationDefaults`.

## Samples + harness workflow
- Sample corpus is under `samples/http-domain`.
- Compile script: `samples/http-domain/compile-samples.ts` emits `*.rexc` and `*.opt.rexc` for `*.rex` only, and intentionally skips `*.test.rex`.
- Harness script: `samples/http-domain/run-sample-tests.ts` treats `*.test.rex` as Rex-authored test docs and runs them via `evaluateSource`.
- Harness value assertions use deep strict equality; include full expected object shape (including explicit `undefined` fields when relevant).

## Change checklists (project-specific)
- If `rex.ohm` changed: run `bun run build:grammar` then `bun test` in `packages/rex-lang`.
- If parser/IR/encoder/interpreter changed: run `bun test` in `packages/rex-lang` and spot-check with `rex --expr ... --ir` (or `bunx @creationix/rex --expr ... --ir`, `npx -y @creationix/rex -- --expr ... --ir`, or `bun run rex:compile --expr ... --ir`).
- If docs examples changed (`language.md`, `rexc-bytecode.md`): run `bun run rex:verify-docs` from root.
- If VS Code grammar/tokenization changed: run `bun test` and `bun run build` in `packages/vscode-rex`.
