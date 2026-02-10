# Rex Core Language

A minimal, reusable expression language that is a superset of JSON for data processing and control flow. Rex can be encoded as compact JSON bytecode (s-expressions encoded as arrays). The core language is domain-agnostic and can be extended for specific use cases.

## Syntax

See [rex.ohm](rex.ohm) for the complete Ohm grammar.

### Expressions

```rex
program   = Expr*
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

1. **Calls** `()` — Code execution, first element is the opcode
2. **Arrays** `[]` — Data lists
3. **Objects** `{}` — Key-value maps

**Separator rules:**
- Whitespace and commas are interchangeable separators
- Trailing commas are allowed
- Comments allowed anywhere whitespace is permitted

```rex
// All valid
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

### Bindings

Variable bindings use `=` syntax:

```rex
x = 42
name = "Alice"
result = (add x 10)
```

### Calls

Function calls are prefix notation with parentheses:

```rex
(add 1 2)
(if condition body... else other-body...)
(map items (mul self 2))
```

## Semantics

### Bytecode Compilation

Rex source compiles to JSON s-expressions. Opcodes are prefixed with `$`:

```rex
(add 1 2)
```

Compiles to:

```json
["$add", 1, 2]
```

### Object Key Semantics

In object literals, bare words are treated as **literal string keys**, not variable references:

```rex
{foo: 1, bar: 2}     // Keys are "foo" and "bar" (strings)
{"foo": 1, bar: 2}   // Same thing
{200: "OK", 404: "Not Found"}  // Number keys are allowed
```

### Variable Scoping

- `bareWord` in value position → variable reference
- `bareWord` in key position → string literal
- `self` → implicit value in current scope (iteration, match, etc.)

## Core Keywords

### Literals

- `true` — Boolean true
- `false` — Boolean false
- `null` — Null value
- `undefined` — Missing/absent value

### Control Flow

| Keyword | Usage | Description |
|---------|-------|-------------|
| `when` | `(when cond body...)` | Execute body if `cond` is not `undefined` |
| `if` | `(if cond body...)` | Truthy check (not `undefined`/`null`/`false`) |
| `else` | inline keyword | Unconditional fallback branch (splits body sections) |
| `else-when` | inline keyword | Chain existence check (splits body sections) |
| `else-if` | inline keyword | Chain truthy check (splits body sections) |
| `match` | `(match expr label body...)` | Multi-branch dispatch by type or value |
| `do` | `(do expr...)` | Sequence multiple expressions |
| `coalesce` | `(coalesce a b c ...)` | First non-undefined value |

**Note:** Inline keywords (`else`, `else-when`, `else-if`) split the body into branches within `when`, `if`, and `match` expressions.

**Examples:**

```rex
// When: check for existence
(when x=(get 'config')
  (process x))

// When with else-when chain
(when x=(get 'primary')
  (use x)
  else-when y=(get 'fallback')
  (use y)
  else
  (use-default))

// If-else chain
(if (gt x 10)
  "large"
  else-if (gt x 5)
  "medium"
  else
  "small")

// Match by type
(match value
  string (upper value)
  number (mul value 2)
  array (count value)
  else undefined)

// Do: sequence expressions
(do
  (set 'x' 10)
  (set 'y' 20)
  (add x y))
```

### Variables

| Keyword | Usage | Description |
|---------|-------|-------------|
| `self` | bare word | Implicit value in current scope |
| `get` | `(get name)` | Read named variable |
| `set` | `(set name value)` | Bind variable (also `name=expr` sugar) |

```rex
// Variable binding
(set 'counter' 42)
x = (get 'counter')

// Implicit self in iteration
(map items (mul self 2))
```

### Container Operations

| Keyword | Usage | Description |
|---------|-------|-------------|
| `read` | `(read root ...keys)` | Read value at path |
| `write` | `(write root ...keys value)` | Set value at path |
| `append` | `(append root ...keys value)` | Append to multi-value path |
| `delete` | `(delete root ...keys)` | Delete at path |
| `has` | `(has root ...keys)` | Check if key exists |
| `missing` | `(missing root ...keys)` | Check if key is absent |
| `read-all` | `(read-all root ...keys)` | All values for multi-value key |
| `count` | `(count root ...keys)` | Length of array/string/object |
| `keys` | `(keys root ...keys)` | Object keys as array |
| `values` | `(values root ...keys)` | Object values as array |
| `entries` | `(entries root ...keys)` | Key-value pairs as `[key, value]` arrays |
| `clear` | `(clear root ...keys)` | Remove all entries |

```rex
obj = {a: 1, b: 2, c: 3}

(read obj 'a')              // 1, equivalent to (obj 'a')
(write obj 'd' 4)           // {a: 1, b: 2, c: 3, d: 4} 
(has obj 'a')               // true
(missing obj 'z')           // true
(keys obj)                  // ["a", "b", "c"]
(count obj)                 // 3
```

### String Operations

| Keyword | Usage | Description |
|---------|-------|-------------|
| `upper` | `(upper s)` | Convert to uppercase |
| `lower` | `(lower s)` | Convert to lowercase |
| `split` | `(split s [sep])` | Split on whitespace or separator |
| `concat` | `(concat a b ...)` | Concatenate strings |
| `join` | `(join sep a b ...)` | Join with separator |
| `starts-with` | `(starts-with s prefix)` | Check if starts with prefix |
| `ends-with` | `(ends-with s suffix)` | Check if ends with suffix |

```rex
(upper "hello")                    // "HELLO"
(split "a,b,c" ",")               // ["a", "b", "c"]
(concat "hello" " " "world")      // "hello world"
(starts-with "hello" "hel")       // true
```

### Arithmetic

| Keyword | Usage | Description |
|---------|-------|-------------|
| `add` | `(add a b ...)` | Sum |
| `sub` | `(sub a b)` | Subtract |
| `mul` | `(mul a b ...)` | Multiply |
| `div` | `(div a b)` | Divide |
| `mod` | `(mod a b)` | Modulo |
| `neg` | `(neg a)` | Negate |

```rex
(add 1 2 3)        // 6
(sub 10 3)         // 7
(mul 2 3 4)        // 24
(div 10 2)         // 5
(mod 10 3)         // 1
(neg 5)            // -5
```

### Comparison

| Keyword | Usage | Description |
|---------|-------|-------------|
| `eq` | `(eq a b)` | Equal |
| `neq` | `(neq a b)` | Not equal |
| `gt` | `(gt a b)` | Greater than |
| `gte` | `(gte a b)` | Greater than or equal |
| `lt` | `(lt a b)` | Less than |
| `lte` | `(lte a b)` | Less than or equal |

```rex
(eq 1 1)           // true
(neq "a" "b")      // true
(gt 5 3)           // true
(lte 2 2)          // true
```

### Boolean Logic

| Keyword | Usage | Description |
|---------|-------|-------------|
| `and` | `(and a b ...)` | Logical AND |
| `or` | `(or a b ...)` | Logical OR |
| `not` | `(not a)` | Logical NOT |

```rex
(and true true false)      // false
(or false false true)      // true
(not false)                // true
```

### Iteration

| Keyword | Usage | Description |
|---------|-------|-------------|
| `map` | `(map iter body)` | Transform each element (`self` = element) |
| `filter` | `(filter iter body)` | Keep elements where body is truthy |
| `each` | `(each iter body)` | Iterate with side effects |

```rex
(map [1 2 3] (mul self 2))                    // [2, 4, 6]
(filter [1 2 3 4] (gt self 2))                // [3, 4]
(each [1 2 3] (print self))                    // side effects
```

### Type Filters

| Keyword | Usage | Description |
|---------|-------|-------------|
| `as-string` | `(as-string expr)` | Pass value if string, else `undefined` |
| `as-number` | `(as-number expr)` | Pass value if number, else `undefined` |
| `as-object` | `(as-object expr)` | Pass value if object, else `undefined` |
| `as-array` | `(as-array expr)` | Pass value if array, else `undefined` |
| `as-bytes` | `(as-bytes expr)` | Pass value if bytes, else `undefined` |

```rex
(as-string "hello")        // "hello"
(as-string 42)             // undefined
(when x=(as-number value)
  (mul x 2))
```

### Match Type Labels

Used as case labels in `match` expressions for type-based dispatch:

| Label | Description |
|-------|-------------|
| `string` | String type |
| `number` | Number type |
| `object` | Object type |
| `array` | Array type |
| `boolean` | Boolean type |
| `bytes` | Bytes type |
| `some` | Any non-undefined value |

```rex
(match value
  string (upper value)
  number (add value 1)
  array (count value)
  bytes (count value)
  boolean (not value)
  else undefined)
```

## Design Philosophy

Rex is designed to be:

1. **Minimal** — Small core with few concepts
2. **Readable** — Clean syntax for humans
3. **Composable** — Build complex logic from simple parts
4. **Portable** — Compiles to JSON (runs anywhere)
5. **Domain-agnostic** — Core language is reusable
6. **Type-aware** — Built-in type filtering and matching

## Extension Points

The core language can be extended with domain-specific keywords and primitives. For example, the routing domain adds:

- HTTP primitives: `headers`, `query`, `cookies`, `method`, `path`, `host`, etc.
- Routing mutations: `rewrite`, `redirect`, `respond`
- Pattern matching: `path-match`, `domain-match`
- Configuration: `config`, `env`

See [language.md](language.md) for the full routing extension.
