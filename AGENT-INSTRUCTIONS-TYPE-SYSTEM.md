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

2. **`/packages/rusty-rex/README.md`** — Architecture overview of the Rust crates

## Prerequisites

1. **`type` and `extern` keywords** — **COMPLETE.** `KwType`, `KwExtern` tokens and `TypeDecl`, `ExternDecl` composite nodes are implemented in the lexer, syntax, and parser. `mut` is contextual after `extern`. `Star` is a valid object key (for `{*: T}` map types). The lowerer ignores `TypeDecl`/`ExternDecl` nodes.

2. **`return` keyword** — **COMPLETE.** `KwReturn` and `ReturnExpr` are implemented. The type checker must handle `ReturnExpr` (see SyntaxKind table below).

3. **No dependency on bytecode or interpreter.** The type checker works on the CST (syntax tree), not bytecode. It does not touch `Value`, the encoder, decoder, or interpreter.

## Working Example: rex-serve

The `packages/rusty-rex/examples/knowledge-base/` directory has a complete rex-serve project with:
- `rex-serve.rexd` — domain interface file (already written, 152 lines)
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

1. **`.rexd` parser** — Parse `.rexd` files into a `DomainSchema` struct. `.rexd` files use standard Rex grammar with `type` and `extern` keywords — parse them with the existing Rex parser, then walk the CST to extract type declarations. See `rex-types.md` for the full syntax.

   **Important:** Function signatures like `extern json.parse(text: string) = some` produce parse errors on the `:` inside the call args (`:` is not an expression operator). This is expected and acceptable — the CST preserves all tokens regardless of parse errors. The `.rexd` walker must extract argument names and types from the raw token sequence inside `CallExpr` nodes, tolerating error tokens.

   **Rest parameters:** The `rex-serve.rexd` file uses `...values: some` for variadic functions (e.g., `extern html(parts: [string], ...values: some) = string`). The lexer tokenizes `...` as `DotDot` + `Dot`. The `.rexd` walker should detect this pattern and mark the parameter as rest/variadic.

2. **Type representation** — An enum:
   ```rust
   enum Type {
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
       Object {                         // all object/map forms unified
           fields: Vec<(String, Type)>, // known fields (empty for pure maps)
           wildcard: Option<Box<Type>>, // None = unknown keys are errors
       },
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
   - `ast.rs` — typed AST wrappers (`BinaryExpr`, `ConditionalExpr`, `ForExpr`, `ObjectExpr`, `ArrayExpr`, `TemplateExpr`, etc.)
   - `syntax.rs` — `SyntaxKind` enum with all node/token types
   - The parser supports `parse_with_cache()` for incremental reparsing

### Actual SyntaxKind values to handle

The inference walk must cover these composite node kinds (from `syntax.rs`):

| SyntaxKind | Rex syntax | Notes |
|------------|-----------|-------|
| `DecimalNumber`, `HexNumber`, `BinaryNumber` | `42`, `0xff`, `0b101` | Literals |
| `DoubleString`, `SingleString` | `"hello"`, `'hello'` | String literals |
| `TemplateLiteral` | `` `hello ${x}` `` | Inside `TemplateExpr` |
| `BinaryExpr` | `a + b`, `a == b` | Binary operators |
| `UnaryExpr` | `-x`, `not x` | Unary operators |
| `AssignExpr` | `x = 42`, `x += 1` | Assignment |
| `CallExpr` | `f(x)`, `json.parse(text)` | Function calls, also type predicates like `number(x)` |
| `NavExpr` | `req.method`, `a.b.c` | Property access |
| `ConditionalExpr` | `when`/`unless`/`else` | Control flow with narrowing |
| `ForExpr` | `for x in items do ... end` | Loop |
| `WhileExpr` | `while cond do ... end` | Loop |
| `ArrayExpr` | `[1, 2, 3]` | Array literal |
| `ArrayComprehension` | `[x * 2 for x in items]` | Array comprehension |
| `ObjectExpr` | `{a: 1, b: 2}` | Object literal (contains `Pair` children) |
| `ObjectComprehension` | `{k: v for k, v in obj}` | Object comprehension |
| `Pair` | `key: value` | Inside `ObjectExpr`/`ObjectComprehension` |
| `TemplateExpr` | `` `hello ${x}` `` | Template literal expression |
| `RangeExpr` | `1..10` | Range |
| `GroupExpr` | `(expr)` | Parenthesized expression |
| `SelfExpr` | `self` | Self reference |
| `Block` | `do ... end` body | Block of statements |
| `Ident` | `x`, `method` | Variable reference |
| `KwTrue`, `KwFalse` | `true`, `false` | Boolean literals |
| `KwNull` | `null` | Null literal |
| `KwNone` | `none` | None literal |
| `ReturnExpr` | `return expr` | Return statement — infer child, type is `never` |
| `TypeDecl` | `type Name = T` | Type alias declaration — skip (only used in `.rexd` parsing) |
| `ExternDecl` | `extern name = T` | Host binding declaration — skip (only used in `.rexd` parsing) |

**Note on AST wrappers:** Most composite nodes have `ast_node!` typed wrappers in `ast.rs` (e.g., `BinaryExpr`, `ConditionalExpr`, `ReturnExpr`). However, `TypeDecl` and `ExternDecl` do **not** have typed wrappers — work with the raw `SyntaxNode` for `.rexd` parsing.

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
11. **String coercion only in template literals** — `+` does NOT coerce; `string + number` is an error. Template literals coerce all types to strings: `none`→`∅`, `true`→`✓`, `false`→`✗`, `null`→`␀`, `Infinity`→`∞`
12a. **No operations on `some`** — arithmetic, comparison, and concatenation on `some` are errors. Must narrow first via type predicates or `when`. Navigation (property read) on `some` is valid → `some | none`.
12b. **Assignability** — `integer` assignable to `number`, `LiteralStr` to `string`, any non-`none` type to `some`, `never` to anything. These are transitive.
13. **`.rexd` files are valid Rex** — they use standard Rex grammar with `type` and `extern` keywords; no special parser mode
14. **`type Name = T`** — defines a named type alias (uppercase by convention)
15. **`extern name = T`** — declares a host-provided binding with a type
16. **`extern mut name = T`** — mutable host binding (`mut` is contextual after `extern`)
17. **`extern name.fn(arg: T) = R`** — function signature with return type
18. **`{*: T}` wildcard key** — only valid inside type expressions, not in regular object literals
19. **Unified Object type** — `Object { fields, wildcard: Option<Type> }`. `{key: T}` = fields + no wildcard, `{*: T}` = no fields + wildcard, `{key: T, *: U}` = fields + wildcard. One match arm, not three.
20. **`for` loop type** — `typeof(body) | none`, not `typeof(body)`. Empty collection = no iterations = `none`.

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

## Template Literal Type Inference

Template literals are already implemented in the compiler. The type checker must handle them:

- `TemplateExpr` always produces `Type::Str`
- Each interpolation is inferred independently — no type errors on interpolated values (template coercion accepts everything except `never`)
- Tagged templates (e.g., `` html`<p>${x}</p>` ``) should resolve the tag function's return type from the domain schema

```rust
SyntaxKind::TemplateExpr => {
    // Infer all interpolation expressions (for side effects / variable tracking)
    // but the template itself always produces a string
    for interpolation in extract_interpolations(node) {
        self.infer_expr(&interpolation);
    }
    Type::Str
}
```

## Return Statement Type Inference

Early return is implemented (`KwReturn` + `ReturnExpr`). The type checker must handle it:

```rust
SyntaxKind::ReturnExpr => {
    // Infer the return value expression if present
    if let Some(value_expr) = ... {
        self.infer_expr(&value_expr);
    }
    Type::Never  // code after return is unreachable
}
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
