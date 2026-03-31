# Instructions: Implement Rex Type Checker CLI

> **Status: COMPLETE.** `rex check` command implemented with auto-discovery of `.rexd` files, colored output, file:line:col diagnostics, and exit code 1 on errors. Integrated with the type checker engine in `typecheck.rs`.

## Goal

Build a `rex check` CLI command that type-checks Rex source files against a `.rexd` domain interface. No user-written type annotations — all types are inferred. The checker outputs errors and warnings to stderr with file/line/column locations. Exit code 0 = clean, 1 = errors found.

This is what agents and developers will use to verify Rex programs are correct before deployment.

## Example Usage

```sh
# Check a single file
rex check routes/api/articles.rex --domain rex-serve.rexd

# Check all .rex files in a directory
rex check routes/ --domain rex-serve.rexd

# Auto-find .rexd (search upward from file)
rex check routes/api/articles.rex
```

Example output:
```
routes/api/articles.rex:11:3: error: Cannot assign integer to res.status (expected integer, got string)
routes/api/articles.rex:22:5: warning: Variable 'record' is assigned but never used
routes/_middleware.rex:13:3: warning: Unknown property 'headrs' on request. Did you mean 'headers'?

2 errors, 1 warning
```

## Prerequisites

1. **Type/extern keywords** — **COMPLETE.** `KwType`, `KwExtern`, `TypeDecl`, `ExternDecl` are implemented.
2. **Type checker engine** — `AGENT-INSTRUCTIONS-TYPE-SYSTEM.md` must be complete. This task builds the CLI wrapper around the engine built there.

This task does NOT depend on the bytecode v2 migration or early return. The type checker works on the CST, not bytecode.

## Key Documents

Read these first:

1. **`/rex-types.md`** — THE SPEC. Read every section. It defines all types, inference rules, narrowing, diagnostics.
2. **`/AGENT-INSTRUCTIONS-TYPE-SYSTEM.md`** — Design decisions and architecture. The type checker engine (types, inference, `.rexd` parsing) is built there — this task wraps it in a CLI.
3. **`/packages/rusty-rex/examples/knowledge-base/rex-serve.rexd`** — Working domain file.
4. **`/packages/rusty-rex/examples/knowledge-base/routes/`** — Rex files to test against.

## Architecture

This task assumes the type checker engine already exists in `crates/rex-core/src/typecheck.rs` (or similar), providing:

- `Type` enum
- `DomainSchema` struct with `parse_rexd()` function
- `typecheck::check(root: &SyntaxNode, schema: &DomainSchema) -> Vec<Diagnostic>` function
- `Diagnostic` struct with `kind`, `span`, and `message`

If any of these don't exist yet, complete `AGENT-INSTRUCTIONS-TYPE-SYSTEM.md` first.

### CLI Command

Add to `crates/rex-cli/src/main.rs`:

```rust
/// Type-check Rex files against a domain interface
Check {
    /// Input file or directory
    input: PathBuf,
    /// Domain interface file (.rexd). Auto-discovered if not specified.
    #[arg(long)]
    domain: Option<PathBuf>,
},
```

### Implementation

1. Find `.rexd` file (explicit `--domain` flag, or search upward from input)
2. Parse `.rexd` → `DomainSchema`
3. For each `.rex` file:
   a. Parse to CST
   b. Run type checker with the domain schema
   c. Collect diagnostics
4. Print diagnostics with file:line:col format
5. Exit 0 if no errors, 1 if errors

### .rexd Auto-Discovery

When `--domain` is not specified, search upward from the input file's directory for any `*.rexd` file:

```rust
fn find_rexd(start: &Path) -> Option<PathBuf> {
    let mut dir = if start.is_file() { start.parent()? } else { start };
    loop {
        for entry in std::fs::read_dir(dir).ok()? {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension() == Some("rexd".as_ref()) {
                return Some(path);
            }
        }
        dir = dir.parent()?;
    }
}
```

### Diagnostics Format

```
file.rex:LINE:COL: error: MESSAGE
file.rex:LINE:COL: warning: MESSAGE
```

The checker needs to convert byte offsets (from CST spans) to line:col. Helper:

```rust
fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in source.char_indices() {
        if i >= offset { break; }
        if ch == '\n' { line += 1; col = 1; } else { col += 1; }
    }
    (line, col)
}
```

## Files to Create/Modify

| File | Action |
|------|--------|
| `crates/rex-cli/src/main.rs` | Add `rex check` command |
| `crates/rex-core/tests/typecheck_cli.rs` | **New** — Integration tests for CLI behavior |

All other files (type enum, inference engine, `.rexd` parser, unit tests) are created by the type system task.

## Test Strategy

### Integration tests

Run the checker on the knowledge-base example:

```rust
#[test]
fn check_knowledge_base() {
    let rexd = std::fs::read_to_string("examples/knowledge-base/rex-serve.rexd").unwrap();
    let schema = parse_rexd(&rexd);

    for entry in walkdir::WalkDir::new("examples/knowledge-base/routes")
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension() == Some("rex".as_ref()))
    {
        let source = std::fs::read_to_string(entry.path()).unwrap();
        let diags = typecheck::check_source(&source, &schema);
        // Print diagnostics for debugging
        for d in &diags {
            eprintln!("{}:{}: {}", entry.path().display(), d.line, d.message);
        }
    }
}
```

### CLI tests

```rust
#[test]
fn cli_check_clean_file_exits_zero() {
    // Run `rex check` on a clean file, assert exit code 0
}

#[test]
fn cli_check_bad_file_exits_one() {
    // Run `rex check` on a file with type errors, assert exit code 1
}

#[test]
fn cli_auto_discovers_rexd() {
    // Run `rex check` without --domain, assert it finds the .rexd file
}
```

## What Success Looks Like

1. `rex check routes/ --domain rex-serve.rexd` runs on the knowledge-base example
2. Known properties resolve correctly (no false positives on `method`, `headers`, `res.status`, etc.)
3. Type predicates narrow correctly (`when number(x) do x + 1 end` — no error)
4. Unknown properties produce warnings with "did you mean" suggestions
5. Wrong argument types produce errors (`json.parse(42)`)
6. Wrong argument counts produce errors
7. Assignment to read-only globals produces errors
8. The exit code is 0 for clean files, 1 for files with errors

## What NOT to Build

- No LSP server — just the CLI command
- No incremental checking — batch mode is fine
- No type annotations in Rex source — inference only
- No modifications to the compiler, interpreter, or bytecode
- No generics or polymorphism — the type system is simple structural types + unions
