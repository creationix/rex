# Rex Core Language

A minimal expression language that is a superset of JSON. Rex adds `()` for code to JSON's `{}` and `[]`. Source compiles to compact JSON bytecode (s-expressions encoded as arrays). The core language is domain-agnostic and can be extended for specific use cases.

## Syntax

See [rex.ohm](packages/rex-lang/rex.ohm) for the complete Ohm grammar.

### Expressions

```
Program   = Expr*
Expr      = LValue "=" Expr | Call | Atom
LValue    = bareWord ("." bareWord)*
Call      = "(" Expr+ ")"
Atom      = String | Number | Bytes | Boolean | Null | Undefined | Array | Object | LValue
```

### Data Types

- **Strings**: Single or double quoted with escape sequences: `"hello"`, `'world'`
- **Numbers**: Decimal, hex, or binary: `42`, `-3.14`, `1e10`, `0xFF`, `-0x20`, `0b1010`
- **Bytes**: Hex byte sequences: `<48 65 6c 6c 6f>`, `<FF, FE, FD>`
- **Booleans**: `true`, `false`
- **Null**: `null`
- **Undefined**: `undefined` (represents missing/absent values)
- **Arrays**: `[1 2 3]`, `[1, 2, 3,]`, `["a" "b" "c"]`
- **Objects**: `{key: value}`, `{a: 1, b: 2}`, `{"key": 3, 200: "OK"}`
- **Bare words**: Identifiers with dashes/underscores: `foo`, `foo-bar`, `some_value`
- **Dotted paths**: Navigation into nested structures: `foo.bar`, `user.name.first`

### Containers

Three container types with flexible separator rules:

1. **Calls** `()` — Code execution, first element is the operator
2. **Arrays** `[]` — Data lists
3. **Objects** `{}` — Key-value maps

Whitespace and commas are interchangeable separators. Trailing commas allowed. Comments allowed anywhere whitespace is permitted.

```rex
[1 2 3]
[1, 2, 3]
[1, 2, 3,]
[ 1 /* comment */ 2 3 ]
```

### Comments

```rex
// Line comment

/* Block comment */

/* Multi-line
   block comment */
```

## Semantics

### Bytecode Format

Rex source compiles to JSON. The fundamental rule:

> **A JSON array whose first element is a `$`-prefixed string is code. Everything else is data.**

```rex
(add 1 2)        → ["$add", 1, 2]       // code
[1, 2, 3]        → [1, 2, 3]            // data
{a: 1}           → {"a": 1}             // data
42               → 42                    // data
"hello"          → "hello"              // data
```

Special values that have no JSON primitive representation:

```rex
self             → ["$self"]
undefined        → ["$undefined"]
```

To embed a data array that happens to start with a `$`-prefixed string, use `literal`:

```rex
(literal ["$not-code" 1 2])  → ["$literal", ["$not-code", 1, 2]]
```

### Places

Places are the central concept in Rex. A `$$`-prefixed array represents a **place** — a reference to a location in data. The same place can be read from or written to depending on context:

```
["$$name"]                    place: variable name
["$$name", "key"]             place: key in variable name
["$$name", "k1", "k2"]       place: k1.k2 in variable name
```

When a place appears in value context, it is **read**. When it appears as the first argument to `$set` or `$delete`, it is **written** or **deleted**:

```json
["$add", ["$$x"], 1]                  // read x, add 1
["$set", ["$$x"], 42]                 // write 42 to x
["$set", ["$$foo", "bar"], 100]       // write 100 to foo.bar
["$delete", ["$$foo", "bar"]]         // delete foo.bar
```

Domain built-ins use the same pattern with a single `$`:

```json
["$headers", "content-type"]                         // read header
["$set", ["$headers", "content-type"], "text/html"]  // write header
```

Single `$` for domain built-ins, `$$` for user variables. Both are navigable, both work as places.

### Dot Navigation

Dots in the source language are path separators. A dotted path compiles to a place with multiple keys:

```rex
x              → ["$$x"]
x.bar          → ["$$x", "bar"]
x.bar.baz      → ["$$x", "bar", "baz"]
self.name      → ["$self", "name"]
headers.host   → ["$headers", "host"]
```

Dashes and underscores remain part of the name:

```rex
foo-bar        → ["$$foo-bar"]
my_var.key     → ["$$my_var", "key"]
```

### Bindings

`lvalue = expr` is sugar for `(set lvalue expr)`. The left side is a dotted path:

```rex
x = 42               → ["$set", ["$$x"], 42]
foo.bar = 100        → ["$set", ["$$foo", "bar"], 100]
(set x 42)           → ["$set", ["$$x"], 42]
(set foo.bar 100)    → ["$set", ["$$foo", "bar"], 100]
```

`$set` returns the value being set, which allows binding in conditions:

```rex
(when x=(get-data)
  (use x))
// x is bound AND the when checks if the result is not undefined

(when obj.key=(lookup 'something')
  (use obj.key))
// writes to obj.key AND checks the result
```

### Navigable Expressions

When a non-keyword bare word or dotted path appears in call position (first element of a `()`), the arguments become additional path keys:

```rex
// Variable in call position
actions = {a: 1, b: 2}
(actions 'a')             → ["$$actions", "a"]

// Dotted path in call position
(config.routes 'api')     → ["$$config", "routes", "api"]

// Domain built-in in call position
(headers 'x-action')      → ["$headers", "x-action"]

// Keyword in call position — normal opcode, not navigation
(add 1 2)                 → ["$add", 1, 2]
```

### Bare Word Resolution

Bare words resolve in this order:

1. **Local variable** (bound by `=` or `set`) → `["$$name", ...]`
2. **Domain built-in** (e.g. `headers` in an HTTP context) → `["$name", ...]`
3. **Reserved keyword** — always resolves to its keyword meaning

Variables shadow domain built-ins. Reserved keywords cannot be shadowed.

### Implicit Self

`self` refers to the implicit value in the current scope. It is set by `when`, `if`, each `match` arm, and other scope-creating forms:

```rex
(when (some-expr)
  (process self))

(when x=(some-expr)
  (process x)              // x and self are both the value
  (process self))
```

Named bindings are aliases for `self` — useful to avoid shadowing in nested scopes.

`self` is navigable like any other place:

```rex
(match user
  object (concat self.name " <" self.email ">")
  else   "unknown")

// self.name → ["$self", "name"]
```

### Object Key Semantics

In object literals, bare words are literal string keys, not variable references:

```rex
{foo: 1, bar: 2}              // keys are "foo" and "bar" (strings)
{"foo": 1, bar: 2}            // same
{200: "OK", 404: "Not Found"} // number keys allowed
```

## Core Keywords

### Control Flow

#### `when` / `if`

`when` checks existence (not `undefined`). `if` checks truthiness (not `undefined`, `null`, or `false`). Both have fixed arity with an optional else branch:

```
["$when", condition, then-expr, else-expr?]
["$if",   condition, then-expr, else-expr?]
```

Multi-expression bodies auto-wrap in `$do`. Inline keywords (`else`, `else-when`, `else-if`) split the body into branches that compile to nested forms:

```rex
(when x=(get-primary)
    (use x)
  else-when y=(get-fallback)
    (use y)
  else
    (use-default))
```

Compiles to:

```json
["$when", ["$set", ["$$x"], ["$get-primary"]],
  ["$use", ["$$x"]],
  ["$when", ["$set", ["$$y"], ["$get-fallback"]],
    ["$use", ["$$y"]],
    ["$use-default"]]]
```

`if` / `else-if` chains work the same way with `$if`.

#### `match`

Dispatches on a value using alternating predicate/body pairs. Each predicate is evaluated with `self` set to the match subject. A predicate matches when it returns non-`undefined`:

```rex
(match value
  string  (handle-string)
  number  (handle-number)
  'GET'   (handle-get)
  42      (handle-42)
  null    (handle-null)
  some    (handle-any)
  else    (fallback))
```

Compiles to:

```json
["$match", ["$$value"],
  ["$as-string"], ["$handle-string"],
  ["$as-number"], ["$handle-number"],
  ["$is", "GET"], ["$handle-get"],
  ["$is", 42], ["$handle-42"],
  ["$is", null], ["$handle-null"],
  ["$self"], ["$handle-any"],
  ["$fallback"]]
```

How match labels compile:

| Match label | Compiled predicate | Meaning |
|---|---|---|
| Type name (`string`, `number`, ...) | `["$as-TYPE"]` | Type check |
| `some` | `["$self"]` | Any non-undefined value |
| Literal (`'GET'`, `42`, `null`, ...) | `["$is", value]` | Equality check |
| `else` | *(trailing body, no predicate)* | Unconditional fallback |

Each branch body has `self` set to the match subject.

#### `do`

Sequences multiple expressions, returns the last:

```rex
(do
  x = 10
  y = 20
  (add x y))
```

```json
["$do", ["$set", ["$$x"], 10], ["$set", ["$$y"], 20], ["$add", ["$$x"], ["$$y"]]]
```

#### `coalesce`

Returns the first non-`undefined` value. `null` is considered a value:

```rex
(coalesce primary fallback "default")
```

```json
["$coalesce", ["$$primary"], ["$$fallback"], "default"]
```

### Variables

| Keyword | Usage | Bytecode |
|---|---|---|
| `self` | bare word | `["$self"]` |
| `set` | `(set lvalue expr)` | `["$set", place, expr]` |

`self` is the implicit value in the current scope. `set` writes to a place and returns the value.

### Containers

| Keyword | Usage | Bytecode |
|---|---|---|
| `delete` | `(delete lvalue)` | `["$delete", place]` |

```rex
obj = {a: 1, b: {c: 2}}

obj.a                     // 1 — read from place
obj.b.c                   // 2 — nested read

obj.d = 4                 // write to place
(delete obj.a)            // delete at place
```

### Arithmetic

| Keyword | Usage | Bytecode |
|---|---|---|
| `add` | `(add a b ...)` | `["$add", a, b, ...]` |
| `sub` | `(sub a b)` | `["$sub", a, b]` |
| `mul` | `(mul a b ...)` | `["$mul", a, b, ...]` |
| `div` | `(div a b)` | `["$div", a, b]` |
| `mod` | `(mod a b)` | `["$mod", a, b]` |
| `neg` | `(neg a)` | `["$neg", a]` |

### Comparison

| Keyword | Usage | Bytecode | Returns |
|---|---|---|---|
| `eq` | `(eq a b)` | `["$eq", a, b]` | `true` / `false` |
| `neq` | `(neq a b)` | `["$neq", a, b]` | `true` / `false` |
| `gt` | `(gt a b)` | `["$gt", a, b]` | `true` / `false` |
| `gte` | `(gte a b)` | `["$gte", a, b]` | `true` / `false` |
| `lt` | `(lt a b)` | `["$lt", a, b]` | `true` / `false` |
| `lte` | `(lte a b)` | `["$lte", a, b]` | `true` / `false` |
| `is` | `(is a b)` | `["$is", a, b]` | `a` if equal, else `undefined` |

`is` is existence-friendly: returns the value on match, `undefined` otherwise. Use `is` with `when` and in match contexts. Use `eq` when you need a boolean.

### Boolean Logic

| Keyword | Usage | Bytecode |
|---|---|---|
| `and` | `(and a b ...)` | `["$and", a, b, ...]` |
| `or` | `(or a b ...)` | `["$or", a, b, ...]` |
| `not` | `(not a)` | `["$not", a]` |

### Type Predicates

| Keyword | With argument | Implicit self |
|---|---|---|
| `string` | `["$as-string", expr]` | `["$as-string"]` |
| `number` | `["$as-number", expr]` | `["$as-number"]` |
| `object` | `["$as-object", expr]` | `["$as-object"]` |
| `array` | `["$as-array", expr]` | `["$as-array"]` |
| `boolean` | `["$as-boolean", expr]` | `["$as-boolean"]` |
| `bytes` | `["$as-bytes", expr]` | `["$as-bytes"]` |

Type predicates return the value if it matches the type, `undefined` otherwise. When the value argument is omitted, `self` is used implicitly.

```rex
// Explicit argument
(string "hello")             // "hello"
(string 42)                  // undefined

// With when
(when x=(number value)
  (add x 1))

// In match (implicit self)
(match value
  string (process self)
  number (add self 1))
```

`is` follows the same implicit-self pattern:

```rex
// Explicit argument
(is method 'GET')            // method if equal, undefined otherwise

// In match (implicit self)
(match method
  'GET'  (handle-get)        // compiles to ["$is", "GET"]
  'POST' (handle-post))
```

### Escaping

| Keyword | Usage | Bytecode |
|---|---|---|
| `literal` | `(literal expr)` | `["$literal", expr]` |

Prevents code interpretation. Use when embedding data that would otherwise be treated as bytecode:

```rex
(literal ["$not-code" 1 2])  // produces the data array ["$not-code", 1, 2]
```

## Reserved Words

All reserved keywords, grouped by category:

**Literals:** `true`, `false`, `null`, `undefined`

**Control flow:** `when`, `if`, `else`, `else-when`, `else-if`, `match`, `do`, `coalesce`

**Variables:** `self`, `set`

**Containers:** `delete`

**Arithmetic:** `add`, `sub`, `mul`, `div`, `mod`, `neg`

**Comparison:** `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `is`

**Boolean:** `and`, `or`, `not`

**Type predicates:** `string`, `number`, `object`, `array`, `boolean`, `bytes`, `some`

**Escaping:** `literal`

## Design Philosophy

Rex is designed to be:

1. **Minimal** — Small core, few concepts, easy to implement
2. **Readable** — Clean syntax for humans
3. **Composable** — Build complex logic from simple parts
4. **Portable** — Compiles to JSON, runs anywhere
5. **Domain-agnostic** — Core language is reusable across problem domains
6. **Uniform** — Places are the single model for reading, writing, and navigating data

## Extension Points

The core language can be extended with domain-specific keywords. Extensions register new bare words as domain built-ins that resolve as navigable places with a single `$` prefix. For example, an HTTP routing domain might add:

- Request fields: `headers`, `query`, `cookies`, `method`, `path`, `host`, `status`
- Mutations: `(set method 'POST')`, `(set path '/new')`
- Pattern matching: `path-match`, `domain-match`
- Configuration: `config`, `env`

Domain extensions do not modify the core grammar or bytecode format — they register navigable places and opcodes that the compiler recognizes.
