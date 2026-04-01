---
description: Rex repo tooling and command conventions.
globs: "*.ts, *.js, *.rs, package.json, Cargo.toml"
alwaysApply: false
---

## Compiler

The Rex compiler is in Rust under `crates/`. Use `cargo` for building and `rex` CLI for compiling/checking.

```sh
rex run -e "1 + 2"
rex check examples/case-studies/gradebook.rex
rex run examples/fibonacci.rex
```

```sh
cargo build
cargo test -p rex-core
cargo install --path crates/rex-cli
```

## VS Code Extension

From `packages/vscode-rex`:

```sh
bun test
bun run build
bun run reinstall
```

## Change Checklist

- After Rust compiler changes, run `cargo test -p rex-core`.
- After VS Code extension grammar changes, run `bun test` and `bun run build` in `packages/vscode-rex`.
- After docs examples change, run `bun run rex:verify-docs` from repo root.

## Documentation

- [docs/language.md](docs/language.md) — Rex syntax and semantics reference
- [docs/rx-format.md](docs/rx-format.md) — RX data format (JSON-compatible, subset of REXC)
- [docs/rexc-bytecode.md](docs/rexc-bytecode.md) — REXC bytecode (RX + language constructs)
- [CONTRIBUTING.md](CONTRIBUTING.md) — repo layout, architecture, development workflow
