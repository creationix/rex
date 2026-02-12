# Rex Core Language

A minimal expression language that is a superset of JSON. Rex adds `()` for code to JSON's `{}` and `[]`. Source compiles to compact JSON bytecode (s-expressions encoded as arrays). The core language is domain-agnostic and can be extended for specific use cases.

## Syntax

See [rex.ohm](packages/rex-lang/rex.ohm) for the complete Ohm grammar.

### Expressions

```
Program   = Expr*
Expr      = Binding | Call | Atom
Binding   = bareWord "=" Expr
Call      = "(" Expr+ ")"
Atom      = String | Number | Bytes | Boolean | Null | Undefined | Array | Object | bareWord
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
- **Bare words**: Identifiers with dots/hyphens: `foo`, `foo.bar`, `some-value`

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

### Bare Word Resolution

Bare words in value position resolve in this order:

1. **Local variable** (bound by `=` or `set`) → `["$get", "name"]`
2. **Domain built-in** (e.g. `headers` in an HTTP context) → `["$name"]`
3. **Reserved keyword** — always resolves to its keyword meaning

Variables shadow domain built-ins. Reserved keywords cannot be shadowed.

### Navigable Expressions

When a non-keyword bare word appears in call position (first element of a `()`), the call compiles to a `$read` expression:

```rex
// Variable in call position
actions = {a: 1, b: 2}
(actions 'a')             → ["$read", ["$get", "actions"], "a"]

// Domain built-in in call position
(headers 'x-action')      → ["$read", ["$headers"], "x-action"]

// Keyword in call position — normal opcode, not $read
(add 1 2)                 → ["$add", 1, 2]
```

Any expression resolving to an object or array can be navigated this way. The compiler emits `$read` wrapping the resolved expression.

### Bindings

`name = expr` is sugar for `(set name expr)`:

```rex
x = 42               → ["$set", "x", 42]
(set x 42)           → ["$set", "x", 42]
x                    → ["$get", "x"]
(get x)              → ["$get", "x"]
```

Variable names in `get` and `set` are always bare words (unquoted identifiers), never expressions.

`$set` returns the value being set, which allows binding in conditions:

```rex
(when x=(read obj 'key')
  (process x))
// x is bound AND the when checks if the result is not undefined
```

### Implicit Self

`self` refers to the implicit value in the current scope. It is set by `when`, `if`, each `match` arm, and other scope-creating forms:

```rex
(when (read obj 'key')
  (process self))

(when x=(read obj 'key')
  (process x)              // x and self are both the value
  (process self))
```

Named bindings are aliases for `self` — useful to avoid shadowing in nested scopes.

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
(when x=(get primary)
    (use x)
  else-when y=(get fallback)
    (use y)
  else
    (use-default))
```

Compiles to:

```json
["$when", ["$set", "x", ["$get", "primary"]],
  ["$use", ["$get", "x"]],
  ["$when", ["$set", "y", ["$get", "fallback"]],
    ["$use", ["$get", "y"]],
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
["$match", ["$get", "value"],
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

Each branch body has `self` set to the match subject. The `else` body also receives the original match subject as `self`.

#### `do`

Sequences multiple expressions, returns the last:

```rex
(do
  x = 10
  y = 20
  (add x y))
```

```json
["$do", ["$set", "x", 10], ["$set", "y", 20], ["$add", ["$get", "x"], ["$get", "y"]]]
```

#### `coalesce`

Returns the first non-`undefined` value. `null` is considered a value:

```rex
(coalesce (get primary) (get fallback) "default")
```

```json
["$coalesce", ["$get", "primary"], ["$get", "fallback"], "default"]
```

### Variables

| Keyword | Usage | Bytecode |
|---|---|---|
| `self` | bare word | `["$self"]` |
| `get` | `(get name)` | `["$get", "name"]` |
| `set` | `(set name value)` | `["$set", "name", value]` |

### Containers

| Keyword | Usage | Bytecode |
|---|---|---|
| `read` | `(read root ...keys)` | `["$read", root, keys...]` |
| `write` | `(write root ...keys value)` | `["$write", root, keys..., value]` |
| `delete` | `(delete root ...keys)` | `["$delete", root, keys...]` |

```rex
obj = {a: 1, b: {c: 2}}

(read obj 'a')            // 1
(read obj 'b' 'c')        // 2
(write obj 'd' 4)         // {a: 1, b: {c: 2}, d: 4}
(delete obj 'a')          // {b: {c: 2}}
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

**Variables:** `self`, `get`, `set`

**Containers:** `read`, `write`, `delete`

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
6. **Type-aware** — Built-in type predicates and matching

## Extension Points

The core language can be extended with domain-specific keywords. Extensions add new bare words that resolve as domain built-ins (after local variables, before reserved keywords). For example, an HTTP routing domain might add:

- Request fields: `headers`, `query`, `cookies`, `method`, `path`, `host`, `status`
- Mutations: `rewrite`, `redirect`, `respond`
- Pattern matching: `path-match`, `domain-match`
- Configuration: `config`, `env`

Domain extensions do not modify the core grammar or bytecode format — they simply register additional bare words that the compiler recognizes.
