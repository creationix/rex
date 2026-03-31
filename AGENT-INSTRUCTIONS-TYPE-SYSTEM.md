# Instructions: Implement Rex Type Checking

## Goal

Implement a type checker for Rex that infers types from `.rexd` domain files and Rex source code, with no user-written type annotations. The type checker should run in the LSP (for editor diagnostics, hover, completions) and optionally as a standalone CLI check.

## Key Documents to Read First

1. **`/rex-types.md`** — The type system spec. Read this thoroughly. It defines:
   - All types: `integer`, `number`, `string`, `boolean`, `null`, `none`, `some`, `unknown`, `never`
   - Container types: `[T]`, `{key: T}`, `{*: T}`, `{key: T, *: U}`
   - Union types: `T | U`
   - `.rexd` file syntax for domain interfaces
   - Inference rules for every expression type
   - Type narrowing via predicates, existence checks, and comparison
   - String coercion rules
   - All diagnostics (errors and warnings)

2. **`/packages/rusty-rex/bytecode-v2.md`** — The bytecode format (for understanding what the compiler produces)

3. **`/packages/rusty-rex/README.md`** — Architecture overview of the Rust crates

## Working Example: rex-serve

The `packages/rusty-rex/examples/knowledge-base/` directory has a complete rex-serve project with:
- `rex-serve.rexd` — domain interface file (already written, 140 lines)
- `routes/` — Rex source files that use the domain

Use this project to test the type checker. The `.rexd` file declares all the globals (`req`, `res`, `method`, `headers`, etc.) and functions (`json.parse`, `db.get`, `fs.read`, etc.) that the Rex files use.

### Example: what the type checker should infer

For `routes/api/articles.rex`:
```rex
when method == "GET" do
  articles = db.list("article:")      // articles: [DbEntry]
  items = [json.parse(a.value) for a in articles]  // items: [some]
  {ok: true, articles: [{slug: a.slug, title: a.title, updated: a.updated} for a in items]}
else when method == "POST" do
  input = json.parse(body)           // input: some
  unless input and input.slug and input.title and input.body do
    // input might be none here
    res.status = 422
    {ok: false, error: "slug_title_body_required"}
  end
  when input and input.slug and input.title and input.body do
    // input: some (narrowed from some | none)
    // input.slug: some | none (property on some)
    record = { ... }
    db.set("article:" + input.slug, json.stringify(record))
    // type error if input.slug is some|none and + expects string
    // BUT: input.slug was checked in the `when` condition above
    res.status = 201
    {ok: true, slug: input.slug}
  end
end
```

### Example diagnostics the checker should produce

```rex
req.headrs                    // warning: Unknown property 'headrs'. Did you mean 'headers'?
json.parse(42)                // error: Expected string for 'text', got integer
res.status = "ok"             // error: Expected integer, got string
req.method = "POST"           // error: Cannot assign to read-only property 'method'
db.list()                     // error: db.list expects 1 argument, got 0
```

## Architecture

### Where the code goes

The type checker should live in `packages/rusty-rex/crates/rex-core/src/` as a new module, or in a new `rex-lsp` crate. The core infrastructure:

1. **`.rexd` parser** — Parse `.rexd` files into a `DomainSchema` struct. The syntax is Rex-like but describes types. See `rex-types.md` for the full syntax.

2. **Type representation** — An enum:
   ```rust
   enum Type {
       Unknown,    // alias for Some | None
       Some,       // opaque defined value
       None,       // absence
       Never,      // unreachable
       Null,
       Bool,
       Int,
       Number,     // int or decimal
       Str,
       LiteralStr(String),  // "GET", "POST", etc.
       Array(Box<Type>),
       Object(Vec<(String, Type)>),  // known fields
       Map(Box<Type>),              // {*: T}
       ObjectMap(Vec<(String, Type)>, Box<Type>),  // {key: T, *: U}
       Union(Vec<Type>),
       Ref(String),  // reference to type alias
   }
   ```

3. **Inference engine** — Walk the CST (rowan syntax tree) top-to-bottom:
   - Seed the type environment from the `.rexd` schema
   - For each expression, compute its type
   - Track variable types through assignments
   - Narrow types through `when`/`unless` branches and type predicates
   - Report errors and warnings

4. **Existing infrastructure to reuse**:
   - `lexer.rs` — logos tokenizer (already handles all Rex tokens)
   - `parser.rs` — rowan CST parser with Pratt precedence
   - `ast.rs` — typed AST wrappers (`BinaryExpr`, `ConditionalExpr`, etc.)
   - `syntax.rs` — `SyntaxKind` enum with all node/token types
   - The parser supports `parse_with_cache()` for incremental reparsing

## Key Design Decisions Already Made

1. **No user annotations** — all types are inferred from domain files + literals + operators
2. **`some`/`none`/`unknown`** — not `any`/`undefined`. `some` = opaque value, `none` = absence, `unknown` = `some | none`
3. **No `any` escape hatch** — `unknown`/`some` must be narrowed via `number()`, `string()`, `when`, etc.
4. **Navigation on `none` → `none`** — not an error, just propagates
5. **Navigation on `some` → `some | none`** — key might not exist
6. **Navigation on known-field object with unknown key → error** (but types as `none`)
7. **Map type `{*: T}`** — lookup returns `T | none`
8. **Mixed `{key: T, *: U}`** — known field returns `T`, unknown key returns `U | none`
9. **Union property access** — resolve on each branch independently, union the results
10. **`none` replaces `undefined`** — the keyword in Rex source is `none`
11. **String coercion** — `none`→`∅`, `true`→`✓`, `false`→`✗`, `null`→`␀`, `Infinity`→`∞`

## Type Narrowing (Critical)

This is the most important part to get right:

### Via type predicates
```rex
when number(x) do    // x: number inside this block
when string(x) do    // x: string inside this block
```

### Via existence
```rex
when x do            // x has none removed from its type
// else branch: x is none
```

### Via comparison
```rex
when method == "GET" do    // method: "GET" (literal type)
```

### Via `and`
```rex
when input and input.slug do
  // input: some (none removed by first check)
  // input.slug: some (none removed by second check)
end
```

## Testing Strategy

1. Write unit tests that parse Rex source, run the type checker, and assert inferred types
2. Run the checker on all `.rex` files in `examples/knowledge-base/routes/`
3. Assert specific diagnostics on intentionally broken examples
4. Test edge cases: nested narrowing, comprehension variable scoping, compound assignment types

## What NOT to Build

- Don't build the LSP transport (tower-lsp) yet — just the type checking engine
- Don't modify the compiler or interpreter — types are purely for tooling
- Don't add type annotations to Rex syntax — inference only
- Don't worry about incremental checking — batch mode is fine for now
