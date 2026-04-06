# Known Issues

## Release Blocker: rex-luajit repo artifacts are not portable

`crates/rex-luajit/librex_luajit.dylib` currently points to an absolute local
path (`/Users/tim/...`), which is machine-specific and non-portable.

**Impact:** fresh clones or CI environments can fail to load LuaJIT native
bindings unless local symlinks are manually repaired.

**Fix:** remove machine-local symlinks from source control and make test/runtime
loading deterministic via one of:

- Build-step generated local symlink in `target` only.
- Runtime loader that resolves platform-specific build outputs.
- A script/Make target that creates correct links for Linux/macOS.

## Release Readiness Gap: no single cross-runtime test entrypoint

The repo has multiple valid test surfaces (Rust workspace, VS Code extension,
rex-ts, LuaJIT), but no single command currently guarantees full release
verification across all runtimes.

**Impact:** release confidence depends on manual orchestration and can miss
migration regressions between crates/packages.

**Fix:** add a root test command (for example via `just`, `make`, or npm/bun
script) that runs at least:

- `cargo test`
- `bun test` in `packages/vscode-rex`
- `bun test` in `packages/rex-ts`
- LuaJIT tests in `crates/rex-luajit`

and use it as the required pre-release gate.

## Typechecker: for..in doesn't narrow element types

When iterating over a typed array with `for..in`, the loop variable isn't narrowed to the element type.

```rex
users: [{ name: str, score: int }] = [
  { name: "Ada" score: 95 }
  { name: "Ben" score: 72 }
]
scores-by-name = { (u.name): u.score for u in users }  // u.name, u.score not resolved
```

**Impact:** Code using typed arrays in comprehensions won't get full type checking on element properties.

**Fix:** `for..in` over `[T]` should bind the loop variable as `T`.
