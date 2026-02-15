# Rex Core Language

A minimal expression language that is a superset of JSON. Rex adds `()` for code to JSON's `{}` and `[]`. Source compiles to compact JSON bytecode (s-expressions encoded as arrays). The core language is domain-agnostic and can be extended for specific use cases.

## Syntax

See [rex.ohm](packages/rex-lang/rex.ohm) for the complete Ohm grammar.

### Expressions

```ebnf
Program   = Expr*
Expr      = LValue "=" Expr | Call | Atom
LValue    = bareWord ("." bareWord)*
Call      = "(" Expr+ ")"
Atom      = String | Number | Boolean | Null | Undefined | Array | Object | LValue
```

### Data Types

- **Strings**: Single or double quoted with escape sequences: `"hello"`, `'world'`
- **Numbers**: Decimal, hex, or binary: `42`, `-3.14`, `1e10`, `0xFF`, `-0x20`, `0b1010`
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

1. **Reserved keyword** — always resolves to its keyword meaning, cannot be shadowed
2. **Local variable** (bound by `=` or `set`) → `["$$name", ...]`
3. **Domain built-in** (e.g. `headers` in an HTTP context) → `["$name", ...]`
4. **Error** — unknown identifier

Local variables shadow domain built-ins.

### Implicit Self

`self` refers to the implicit value in the current scope. It is set by `when` and `unless` when the condition is evaluated:

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
self.name      → ["$self", "name"]
self.email     → ["$self", "email"]
```

### Object Key Semantics

In object literals, bare words are literal string keys, not variable references:

```rex
{foo: 1, bar: 2}              // keys are "foo" and "bar" (strings)
{"foo": 1, bar: 2}            // same
{200: "OK", 404: "Not Found"} // number keys allowed
```

## Core Keywords

All operators in Rex work on a single principle: operations either succeed and return a value, or fail and return `undefined`. Control flow checks for existence (defined vs `undefined`). There is no separate concept of truthiness.

### Control Flow

#### `when` / `unless`

`when` runs its body if the condition is defined (not `undefined`). `unless` runs its body if the condition is `undefined`. Both take a condition, a then-expression, and an optional else-expression:

```
["$when",   condition, then-expr, else-expr?]
["$unless", condition, then-expr, else-expr?]
```

Both set `self` to the condition value. Use `do` for multi-expression bodies. Chain by nesting:

```rex
(when x=(get-primary)
  (use x)
  (when y=(get-fallback)
    (use y)
    (use-default)))
```

Compiles to:

```json
["$when", ["$set", ["$$x"], ["$get-primary"]],
  ["$use", ["$$x"]],
  ["$when", ["$set", ["$$y"], ["$get-fallback"]],
    ["$use", ["$$y"]],
    ["$use-default"]]]
```

`unless` handles the negated case:

```rex
(unless (string value)
  (handle-non-string))
```

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

### Short-Circuit Operators

Both are variadic and return actual values (not booleans):

| Keyword | Usage | Returns |
|---|---|---|
| `alt` | `(alt a b c ...)` | First non-`undefined` value |
| `all` | `(all a b c ...)` | Last value if all defined, first `undefined` otherwise |

`alt` is nullish coalescing — `null` is a value, not absence:

```rex
(alt user.preferred-name user.name "anonymous")
```

`all` is like optional chaining — proceed only if everything exists:

```rex
(all user user.name user.email)   // user.email if all exist
```

### Place Operations

| Keyword | Usage | Bytecode |
|---|---|---|
| `set` | `(set lvalue expr)` | `["$set", place, expr]` |
| `delete` | `(delete lvalue)` | `["$delete", place]` |

`set` writes to a place and returns the value. `delete` removes a key from a place.

```rex
obj = {a: 1, b: {c: 2}}

obj.a                     // 1 — read from place
obj.b.c                   // 2 — nested read
obj.d = 4                 // write to place
(delete obj.a)            // delete at place
```

### Comparison

All comparisons return the first argument if the comparison succeeds, `undefined` otherwise:

| Keyword | Usage | Returns |
|---|---|---|
| `eq` | `(eq a b)` | `a` if a = b, else `undefined` |
| `neq` | `(neq a b)` | `a` if a ≠ b, else `undefined` |
| `gt` | `(gt a b)` | `a` if a > b, else `undefined` |
| `gte` | `(gte a b)` | `a` if a ≥ b, else `undefined` |
| `lt` | `(lt a b)` | `a` if a < b, else `undefined` |
| `lte` | `(lte a b)` | `a` if a ≤ b, else `undefined` |

Comparisons compose naturally with `when` and `all`:

```rex
// Branch on comparison
(when (gt age 18)
  (allow self))

// Combine comparisons
(when (all (gt age 18) (lt age 65))
  (process self))
```

### Boolean / Bitwise

Value operators that work on booleans and numbers. These are NOT logical operators — they operate on the values themselves:

| Keyword | Usage | Booleans | Numbers |
|---|---|---|---|
| `and` | `(and a b ...)` | Boolean AND | Bitwise AND |
| `or` | `(or a b ...)` | Boolean OR | Bitwise OR |
| `not` | `(not a)` | Boolean NOT | Bitwise NOT |
| `xor` | `(xor a b ...)` | Boolean XOR | Bitwise XOR |

```rex
// Boolean algebra
(and true false)         // false
(or false true)          // true
(not true)               // false

// Bitwise operations
(and 0xFF 0x0F)          // 15
(or 0x0F 0xF0)           // 255
(xor 0xFF 0x0F)          // 240

// Computing with boolean data
user.can-edit = (and user.is-admin (not user.is-suspended))
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

### Type Predicates

Type predicates return the value if it matches the type, `undefined` otherwise:

| Keyword | Usage | Returns |
|---|---|---|
| `string` | `(string expr)` | `expr` if string, else `undefined` |
| `number` | `(number expr)` | `expr` if number, else `undefined` |
| `object` | `(object expr)` | `expr` if object, else `undefined` |
| `array` | `(array expr)` | `expr` if array, else `undefined` |
| `boolean` | `(boolean expr)` | `expr` if boolean, else `undefined` |

```rex
// Type check with when
(when (string value)
  (process self))

// Type-based dispatch with chained when
(when (string value)
  (handle-string)
  (when (number value)
    (handle-number)
    (handle-other)))
```

## Reserved Words

All reserved keywords, grouped by category:

**Literals:** `true`, `false`, `null`, `undefined`

**Control flow:** `when`, `unless`, `do`

**Short-circuit:** `alt`, `all`

**Place operations:** `self`, `set`, `delete`

**Comparison:** `eq`, `neq`, `gt`, `gte`, `lt`, `lte`

**Boolean / Bitwise:** `and`, `or`, `not`, `xor`

**Arithmetic:** `add`, `sub`, `mul`, `div`, `mod`, `neg`

**Type predicates:** `string`, `number`, `object`, `array`, `boolean`

## Design Philosophy

Rex is designed to be:

1. **Minimal** — Small core, few concepts, easy to implement
2. **Readable** — Clean syntax for humans
3. **Composable** — Build complex logic from simple parts
4. **Portable** — Compiles to JSON, runs anywhere
5. **Domain-agnostic** — Core language is reusable across problem domains
6. **Uniform** — Places are the single model for reading, writing, and navigating data. Operations return value or `undefined`. Control flow checks existence. One model, not two.

## Extension Points

The core language can be extended with domain-specific keywords. Extensions register new bare words as domain built-ins that resolve as navigable places with a single `$` prefix. For example, an HTTP routing domain might add:

- Request fields: `headers`, `query`, `cookies`, `method`, `path`, `host`, `status`
- Mutations: `(set method 'POST')`, `(set path '/new')`
- Pattern matching: `path-match`, `domain-match`
- Configuration: `config`, `env`

Domain extensions do not modify the core grammar or bytecode format — they register navigable places and opcodes that the compiler recognizes.
