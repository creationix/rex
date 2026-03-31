# Instructions: Implement Rex Type Checker CLI

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

## Key Documents

Read these first:

1. **`/rex-types.md`** — THE SPEC. Read every section. It defines all types, inference rules, narrowing, diagnostics.
2. **`/AGENT-INSTRUCTIONS-TYPE-SYSTEM.md`** — Design decisions and architecture overview.
3. **`/packages/rusty-rex/examples/knowledge-base/rex-serve.rexd`** — Working domain file.
4. **`/packages/rusty-rex/examples/knowledge-base/routes/`** — Rex files to test against.

## Architecture

### New module: `crates/rex-core/src/typecheck.rs`

The type checker is a single-pass CST walk. It does NOT use the bytecode — it works directly on the parsed syntax tree (rowan CST).

```
.rex source → lexer → parser → CST → type checker → diagnostics
                                        ↑
                               .rexd → domain schema
```

### Types

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Some,                              // opaque defined value
    None,                              // absence
    Never,                             // unreachable (return, break)
    Null,
    Bool,
    Int,
    Number,                            // int or decimal
    Str,
    LiteralStr(String),                // "GET", "POST", etc.
    Array(Box<Type>),                  // [T]
    Object(Vec<(String, Type)>),       // {key: T, ...}
    Map(Box<Type>),                    // {*: T}
    ObjectMap(Vec<(String, Type)>, Box<Type>),  // {key: T, *: U}
    Union(Vec<Type>),                  // T | U
}
```

Helper constructors:
```rust
impl Type {
    fn unknown() -> Type { Type::Union(vec![Type::Some, Type::None]) }
    fn is_none(&self) -> bool { matches!(self, Type::None) }
    fn remove_none(&self) -> Type { /* strip None from unions */ }
    fn add_none(&self) -> Type { /* add None to type if not present */ }
    fn display(&self) -> String { /* human-readable type string */ }
}
```

### Domain Schema

Parse `.rexd` files into:

```rust
pub struct DomainSchema {
    pub type_aliases: HashMap<String, Type>,
    pub globals: HashMap<String, GlobalEntry>,
    pub functions: HashMap<String, FunctionSig>,  // keyed by dot path: "json.parse"
}

pub struct GlobalEntry {
    pub ty: Type,
    pub mutable: bool,
    pub doc: Option<String>,
}

pub struct FunctionSig {
    pub args: Vec<(String, Type)>,
    pub returns: Type,  // Type::None if no return annotation
    pub doc: Option<String>,
}
```

The `.rexd` parser needs to handle:
- `Name = Type` → type alias
- `name: Type` → global with simple type
- `name = { fields }` → global with structural type
- `mut name = { fields }` → mutable global
- `name.path(arg: Type, ...): ReturnType` → function
- `// comment` → doc string (from line above declaration)

### Type Environment

```rust
struct TypeEnv {
    schema: DomainSchema,
    scopes: Vec<HashMap<String, Type>>,  // stack of variable scopes
    diagnostics: Vec<Diagnostic>,
}

struct Diagnostic {
    kind: DiagnosticKind,  // Error or Warning
    span: (usize, usize), // byte offsets in source
    message: String,
}
```

### Inference Walk

Walk the CST using the existing typed AST wrappers in `ast.rs`. For each node:

```rust
fn infer_expr(&mut self, node: &SyntaxNode) -> Type {
    match node.kind() {
        // Literals
        SyntaxKind::DecimalNumber => { /* check for dot/e → Number, else Int */ }
        SyntaxKind::DoubleString | SyntaxKind::SingleString => Type::Str,
        SyntaxKind::KwTrue | SyntaxKind::KwFalse => Type::Bool,
        SyntaxKind::KwNull => Type::Null,
        SyntaxKind::KwNone => Type::None,

        // Expressions
        SyntaxKind::BinaryExpr => self.infer_binary(node),
        SyntaxKind::UnaryExpr => self.infer_unary(node),
        SyntaxKind::AssignExpr => self.infer_assign(node),
        SyntaxKind::CallExpr => self.infer_call(node),
        SyntaxKind::NavExpr => self.infer_nav(node),
        SyntaxKind::ConditionalExpr => self.infer_conditional(node),
        SyntaxKind::ForExpr => self.infer_for(node),
        SyntaxKind::WhileExpr => self.infer_while(node),
        SyntaxKind::ArrayExpr => self.infer_array(node),
        SyntaxKind::ObjectExpr => self.infer_object(node),
        SyntaxKind::Ident => self.lookup_var(text),

        // ...etc
    }
}
```

### Key Inference Rules

**Assignment:**
```rust
fn infer_assign(&mut self, node: &SyntaxNode) -> Type {
    let rhs_type = self.infer_expr(rhs);
    self.set_var(name, rhs_type.clone());
    rhs_type
}
```

**Binary operators:**
```rust
fn infer_binary(&mut self, node: &SyntaxNode) -> Type {
    let (lhs, op, rhs) = ...;
    let lt = self.infer_expr(lhs);
    let rt = self.infer_expr(rhs);
    match op {
        "+" => {
            if lt.is_str() || rt.is_str() { Type::Str }
            else if lt.is_int() && rt.is_int() { Type::Int }
            else if lt.is_numeric() && rt.is_numeric() { Type::Number }
            else { self.error("cannot add ..."); Type::Some }
        }
        "==" | "!=" | ">" | ">=" | "<" | "<=" => {
            // Returns left type | none
            Type::Union(vec![lt, Type::None])
        }
        // ...
    }
}
```

**Navigation (property access):**
```rust
fn infer_nav(&mut self, node: &SyntaxNode) -> Type {
    let base_type = self.infer_expr(base);
    let key = key_text;
    self.resolve_property(&base_type, key)
}

fn resolve_property(&mut self, ty: &Type, key: &str) -> Type {
    match ty {
        Type::Object(fields) => {
            if let Some(ft) = fields.iter().find(|(k,_)| k == key) {
                ft.1.clone()
            } else {
                self.error(format!("Unknown property '{key}'"));
                Type::None
            }
        }
        Type::Map(vt) => vt.add_none(),  // T | none
        Type::ObjectMap(fields, fallback) => {
            if let Some(ft) = fields.iter().find(|(k,_)| k == key) {
                ft.1.clone()
            } else {
                // Known field miss → warning, but map fallback provides type
                self.warning(format!("Unknown property '{key}'"));
                fallback.add_none()
            }
        }
        Type::Some => Type::Union(vec![Type::Some, Type::None]),
        Type::None => Type::None,
        Type::Union(branches) => {
            // Resolve on each branch, union results
            let results: Vec<Type> = branches.iter()
                .map(|b| self.resolve_property(b, key))
                .collect();
            Type::Union(results).simplify()
        }
        _ => {
            self.warning(format!("Cannot access property on {}", ty.display()));
            Type::None
        }
    }
}
```

**Type narrowing in `when`:**
```rust
fn infer_conditional(&mut self, node: &SyntaxNode) -> Type {
    let cond_type = self.infer_expr(cond);

    // Check if condition is a type predicate: number(x), string(x)
    if let Some((predicate, var_name)) = self.extract_predicate(cond) {
        // Then branch: var is narrowed to the predicate type
        self.push_scope();
        self.set_var(var_name, predicate_type);
        let then_type = self.infer_block(then_block);
        self.pop_scope();

        // Else branch: var type has predicate removed
        // ...
    } else {
        // Then branch: condition has none removed
        self.push_scope();
        self.narrow_from_condition(cond, /* remove none */);
        let then_type = self.infer_block(then_block);
        self.pop_scope();

        // Else branch: condition IS none
        // ...
    }
}
```

**Function calls:**
```rust
fn infer_call(&mut self, node: &SyntaxNode) -> Type {
    // Check if callee is a known function from schema
    let func_path = extract_function_path(node); // e.g., "json.parse"
    if let Some(sig) = self.schema.functions.get(&func_path) {
        // Check arg count
        if args.len() != sig.args.len() {
            self.error(format!("{} expects {} args, got {}", func_path, sig.args.len(), args.len()));
        }
        // Check arg types
        for (i, (arg_name, expected_type)) in sig.args.iter().enumerate() {
            if let Some(actual) = args.get(i) {
                let actual_type = self.infer_expr(actual);
                if !actual_type.is_assignable_to(expected_type) {
                    self.error(format!("Expected {} for '{}', got {}", expected_type.display(), arg_name, actual_type.display()));
                }
            }
        }
        sig.returns.clone()
    } else {
        Type::Some  // unknown function → some
    }
}
```

### .rexd Parser

The `.rexd` format uses Rex-like syntax. You can either:

1. **Reuse the Rex parser** — parse the `.rexd` file as Rex source, then walk the CST to extract type declarations. This works because `.rexd` syntax is a subset of Rex syntax (`Name = { ... }`, `name: Type`, `name.path(args): Type`).

2. **Write a dedicated parser** — simpler, less code, but doesn't reuse infrastructure.

Recommended: option 1. Parse with the existing Rex parser, then walk the CST looking for assignment patterns and function-call-like patterns that represent type declarations.

Type syntax parsing (inside `.rexd`):
- `string`, `number`, `integer`, `boolean`, `null`, `none`, `some`, `unknown`, `never` → primitive types
- `"GET"` → `LiteralStr("GET")`
- `[T]` → `Array(T)`
- `{key: T, ...}` → `Object(fields)`
- `{*: T}` → `Map(T)`
- `{key: T, *: U}` → `ObjectMap(fields, U)`
- `T | U` → `Union([T, U])`
- `Name` (uppercase) → resolve from type_aliases

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

Implementation:
1. Find `.rexd` file (explicit `--domain` flag, or search upward from input)
2. Parse `.rexd` → `DomainSchema`
3. For each `.rex` file:
   a. Parse to CST
   b. Run type checker with the domain schema
   c. Collect diagnostics
4. Print diagnostics with file:line:col format
5. Exit 0 if no errors, 1 if errors

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
| `crates/rex-core/src/typecheck.rs` | **New** — Type enum, DomainSchema, TypeEnv, inference walk, diagnostics |
| `crates/rex-core/src/rexd.rs` | **New** — `.rexd` parser (or include in typecheck.rs) |
| `crates/rex-core/src/lib.rs` | Add `pub mod typecheck;` (and `pub mod rexd;` if separate) |
| `crates/rex-cli/src/main.rs` | Add `rex check` command |
| `crates/rex-core/tests/typecheck.rs` | **New** — Type checker tests |

## Test Strategy

### Unit tests (in `typecheck.rs` or `tests/typecheck.rs`)

```rust
fn check(source: &str, rexd: &str) -> Vec<Diagnostic> {
    let schema = parse_rexd(rexd);
    let tokens = lexer::lex(source);
    let (green, _) = parser::parse(source, &tokens);
    let root = SyntaxNode::new_root(green);
    typecheck::check(&root, &schema)
}

#[test]
fn infer_integer() {
    let diags = check("x = 42", "");
    assert!(diags.is_empty());
}

#[test]
fn error_on_bad_arg_type() {
    let diags = check(
        "json.parse(42)",
        "json.parse(text: string): some"
    );
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("Expected string"));
}

#[test]
fn narrowing_removes_none() {
    let diags = check(
        "x = headers.foo\nwhen x do\n  x + 1\nend",
        "headers: {*: string}"
    );
    // x is string | none, narrowed to string inside when
    // string + integer → error (can't add string and integer)
    assert!(diags.iter().any(|d| d.message.contains("add")));
}
```

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
