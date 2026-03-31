# Rex Type System

Types are inferred from declarations (`type`, `extern`), literals, operators, and type predicates. The type system exists purely for tooling — the compiler and interpreter are untyped.

## Declarations

Rex has two declaration keywords: `type` and `extern`. These are part of the Rex grammar and can appear in any `.rex` or `.rexd` file.

### `type` — define a named type alias

```rex
type Headers = {*: string | [string]}
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE" | "PATCH"
type FileMeta = {size: integer, modified: integer}
```

`type` followed by an identifier, `=`, and a type expression. By convention, type names are uppercase. The compiler and interpreter ignore type declarations — they are consumed only by the type checker.

### `extern` — declare a host-provided binding

```rex
// Read-only binding, all fields read-only
extern req = {
  method: HttpMethod
  path: string | [string]
  headers: Headers
  query: {*: string | [string]}
  cookies: {*: string}
  ip: string
  body: string
}

// Per-field mutability
extern res = {
  mut status: integer
  mut headers: {mut *: string | [string]}
  body: string          // read-only
}

// Reassignable binding
extern mut status = integer

// Read-only globals
extern config = unknown
extern secrets = {*: string}
```

`extern` declares a host-provided binding with a type. The host populates these before the Rex program runs. Everything is read-only by default.

### Mutability (`mut`)

`mut` is a property of a binding — it can appear on the top-level `extern` or on individual fields inside an object type. `mut` is contextual — only recognized after `extern` or before a field name in a type expression, not a standalone keyword.

| Declaration | Meaning |
|---|---|
| `extern name = T` | Read-only binding, read-only fields |
| `extern mut name = T` | Binding is reassignable (can do `name = new_value`) |
| `mut field: T` inside object type | Field is writable (can do `obj.field = value`) |
| `field: T` inside object type | Field is read-only |
| `{mut *: T}` | Map entries are writable (can do `obj.key = value` for any key) |
| `{*: T}` | Map entries are read-only |

`mut` on a field controls writes to that field. `mut` on the binding controls reassignment of the whole binding. They are independent:

```rex
// Can write res.status and res.headers.x-foo, but not res.body
extern res = {
  mut status: integer
  mut headers: {mut *: string | [string]}
  body: string
}

res.status = 404                    // valid — status is mut
res.headers.x-request-id = "abc"   // valid — headers is mut, map entries are mut
res.body = "hello"                  // error — body is not mut
res = {status: 500}                // error — binding is not mut

// Can reassign the whole status binding
extern mut status = integer
status = 404                        // valid — binding is mut
```

### Function signatures (`extern` with call shape)

```rex
extern json.parse(text: string) = some
extern json.stringify(value: some) = string
extern log.info(message: some)
extern res.rewrite(url: string) = never
extern res.redirect(url: string, status: integer) = never
```

`extern` with a call expression on the left declares a function signature. Return type follows `=`. Functions without `= ReturnType` return `none`.

### Comments as documentation

`//` comments immediately above a declaration are extracted as hover documentation:

```rex
// Client IP address from the connecting socket or X-Forwarded-For header
extern ip = string
```

---

## Domain Interface Files (`.rexd`)

A `.rexd` file is a normal `.rex` file that contains only `type` and `extern` declarations. The LSP searches upward from the open file for `*.rexd` files and loads their declarations into the project-wide type environment.

`.rexd` files describe the developer-facing API for a domain. The names match what the programmer writes in Rex code (`req.headers`, `res.status`). The compiler maps these to internal short ref codes via a separate host-provided mapping — the developer never sees or writes short codes.

See `packages/rusty-rex/examples/knowledge-base/rex-serve.rexd` for a complete example.

---

## Type Summary

### Scalar types

| Type | Description |
|------|-------------|
| `integer` | Integer value (`42`, `-1`, `0`) |
| `number` | Any numeric value — integer or decimal (`3.14`, `1e10`) |
| `boolean` | `true` or `false` |
| `string` | String value (`"hello"`) |
| `null` | The null value |
| `none` | Absence of value — missing keys, failed lookups, deletion tombstones |
| `"GET"` | String literal type — only this exact value |

### Container types

| Type | Description |
|------|-------------|
| `[T]` | Array of `T` |
| `{key: T, ...}` | Object with known fields. Unknown key access is an error. |
| `{*: T}` | Map — any string key, lookup returns `T \| none` |
| `{key: T, *: U}` | Object with known fields and wildcard fallback for unknown keys (`U \| none`) |

Internally, all three object forms are a single type: an object with known fields and an optional wildcard. `{key: T}` is `{key: T, *: never}` (unknown keys are errors). `{*: T}` is `{*: T}` with no known fields. This simplifies the type checker — one match arm, not three.

### Meta types

| Type | Description |
|------|-------------|
| `some` | A value exists but its type is opaque — must narrow before use |
| `T \| U` | Union — value can be `T` or `U` |
| `unknown` | Alias for `some \| none` |
| `never` | Function does not return (throws or diverges) |
| `Name` | Reference to a type alias defined with `type` |

### Type expression syntax

Inside `type` and `extern` declarations, the right-hand side of `=` is a type expression. Type expressions reuse Rex value syntax (`{}`, `[]`, `|`, identifiers, string literals) but are interpreted as types:

| Syntax          | Meaning                                             |
|-----------------|-----------------------------------------------------|
| `string`        | String value                                        |
| `number`        | Any numeric value (integer or decimal)              |
| `integer`       | Integer only                                        |
| `boolean`       | `true` or `false`                                   |
| `null`          | The null value                                      |
| `some`          | A value exists but type is opaque — must narrow before use. Does NOT include `none`. |
| `none`          | No value / tombstone. Navigation on `none` produces `none`. |
| `unknown`       | Alias for `some \| none` — a value might or might not exist. |
| `never`         | Function does not return (throws or diverges)       |
| `"GET"`         | String literal type — only this exact string        |
| `[T]`           | Array of `T`                                        |
| `{key: T, ...}` | Object with known fields                           |
| `{*: T}`        | Map — any string key accepted, lookup returns `T \| none` (key may not exist). The `*` wildcard key is only valid in type expressions, not in regular object literals. |
| `{key: T, *: U}` | Object with known fields (exact type) and a wildcard fallback (`U \| none`) for other keys |
| `T \| U`        | Union — value can be `T` or `U`. Uses the `\|` operator syntax but interpreted as union in type context. |
| `Name`          | Reference to a type alias defined with `type`       |

### `some`, `none`, and `unknown`

Three primitive concepts for presence and absence:

- **`some`** — a value exists, but we don't know its type. Must narrow with type predicates before using in operations. Navigation on `some` produces `some | none` (the key might not exist).
- **`none`** — no value. The result of missing properties, failed comparisons, deletion tombstones. Navigation on `none` produces `none`.
- **`unknown`** — alias for `some | none`. Might be a value, might be absent. Common for map lookups and opaque domain fields.

```rex
extern config = some                 // definitely a value, type opaque
config.timeout                       // some | none (key may not exist)

cookies.session                      // string | none (map lookup)

when cookies.session do
  // cookies.session: string (none removed — value exists)
end

extern data = unknown                // same as some | none
when data do
  // data: some (none removed — value exists)
  when number(data) do
    // data: number (narrowed further)
    data + 1                         // valid
  end
end
```

---

## Assignability

A value of type `A` is assignable to a slot expecting type `B` when:

| A | B | Assignable? |
|---|---|-------------|
| `integer` | `number` | Yes — `integer` is a subtype of `number` |
| `LiteralStr(s)` | `string` | Yes — a literal string is a subtype of `string` |
| any type except `none` | `some` | Yes — `some` means "any defined value" |
| any type | `unknown` | Yes — `unknown` is `some \| none` |
| `never` | any type | Yes — `never` is the bottom type |
| `T` | `T \| U` | Yes — `T` is a member of the union |

These rules are transitive: `integer` is assignable to `number`, and `number` is assignable to `some`, so `integer` is assignable to `some`.

### Operations on `some`

No arithmetic, comparison, concatenation, or property-write operations are valid on `some`. The value must first be narrowed to a concrete type using type predicates (`number()`, `string()`, etc.) or `when` guards. Navigation (property read) on `some` is valid and produces `some | none`.

```rex
extern data = some
data + 1              // error: cannot add some and integer — narrow first
data.foo              // valid: some | none

when number(data) do
  data + 1            // valid: number + integer = number
end
```

---

## Type Inference

The type checker walks the program top-to-bottom, inferring a type for every expression.

### Literals

| Expression       | Type                      |
|------------------|---------------------------|
| `42`             | `integer`                 |
| `3.14`, `314e-2` | `number`                  |
| `"hello"`        | `string`                  |
| `true`, `false`  | `boolean`                 |
| `null`           | `null`                    |
| `none`           | `none`                    |
| `[1, 2, 3]`      | `[integer]`               |
| `{a: 1, b: "x"}` | `{a: integer, b: string}` |

### Variables

Assignment creates a variable with the type of the right-hand side:

```rex
x = 42          // x: integer
name = "Ada"    // name: string
items = [1 2 3] // items: [integer]
```

Compound assignment preserves type:

```rex
x = 0       // x: integer
x += 1      // still integer (integer + integer = integer)
```

### String coercion

String coercion applies only in **template literals** (`` `hello ${expr}` ``). The `+` operator does **not** trigger coercion — `string + number` is a type error. Use a template literal to convert non-string values to strings.

| Value | String form | Notes |
|-------|-------------|-------|
| `string` | as-is | |
| `integer` | decimal digits | `42` → `"42"` |
| `number` | decimal | `3.14` → `"3.14"` |
| `boolean` | `✓` or `✗` | U+2713 / U+2717 |
| `null` | `␀` | U+2400 |
| `none` | `∅` | U+2205 |
| `NaN` | `NaN` | |
| `Infinity` | `∞` | U+221E |
| `-Infinity` | `-∞` | |
| `array` | JSON-like | `"[1, 2, 3]"` |
| `object` | JSON-like | `"{a: 1}"` |

This only applies to template literal interpolation. The `+` operator, JSON serialization, comparison, and type checking use the actual values.

### Arithmetic operators

| Expression                         | Type                                                                         |
|------------------------------------|------------------------------------------------------------------------------|
| `integer + integer`                | `integer`                                                                    |
| `number + number`                  | `number`                                                                     |
| `integer + number`                 | `number` (`integer` widens to `number`)                                      |
| `string + string`                  | `string` (concatenation)                                                     |
| `number + string`                  | error: cannot add number and string (use template literals for coercion)     |
| `some + T`                         | error: cannot use `some` in arithmetic — narrow first                        |
| `a - b`, `a * b`, `a / b`, `a % b` | `number` (or `integer` if both integer, except `/` which is always `number`) |
| `-a`                               | same as `a`                                                                  |

### Comparison operators

Comparisons return the left-hand value on success, `none` on failure:

```rex
x > 10    // type: typeof(x) | none
a == b    // type: typeof(a) | none
```

This means:

```rex
score = x > 10    // score: number | none  (if x: number)
```

### Logical operators

Rex uses existence-based logic, not boolean logic. `and` binds tighter than `or`.

| Expression | Type                     | Semantics                          |
|------------|--------------------------|------------------------------------|
| `a or b`   | `typeof(a) \| typeof(b)` | First defined value                |
| `a and b`  | `typeof(b) \| none`      | Second value if first is defined   |

The `or` pattern is commonly used for defaults:

```rex
max = max or 100    // max: number (if max: number | none from domain)
```

### Control flow

**When / Unless:**

```rex
when cond do body end           // type: typeof(body) | none
when cond do a else b end       // type: typeof(a) | typeof(b)
unless cond do body end         // type: typeof(body) | none
```

**Return:**

```rex
return expr                     // type: never (code after return is unreachable)
return                          // type: never (bare return, value is none)
```

**Loops:**

```rex
for x in items do body end      // type: typeof(body) | none
while cond do body end          // type: typeof(body) | none
```

**Comprehensions produce arrays:**

```rex
[x * 2 for x in items]         // type: [number] (if items: [number])
{k: v for k, v in obj}         // type: {*: typeof(v)}
```

### Property access

Accessing a property on a typed value narrows by lookup:

```rex
req.method        // HttpMethod → "GET" | "POST" | ...
req.headers       // Headers → {*: string | [string]}
req.headers.host  // string | [string] (from map value type)
```

For objects with known fields, accessing a known field returns its type. Accessing an unknown field is an **error** (but still types as `none`):

```rex
point = {x: 1, y: 2}
point.x           // integer
point.z           // error: unknown property 'z' on {x: integer, y: integer}
                  // type: none
```

For maps (`{*: V}`), any string key is accepted but the key might not exist, so the result is `V | none`:

```rex
cookies = req.cookies    // {*: string}
cookies.session          // string | none (key may not exist)

when cookies.session do
  // cookies.session: string (narrowed — none removed)
end
```

#### Property access on unions

When a type is a union, property access is resolved on each branch independently and the results are unioned:

```rex
// Given: value: {a: number, b: string} | {*: boolean}
value.a
  // Left branch:  {a: number, b: string}.a → number
  // Right branch: {*: boolean}.a → boolean | none  (map allows any key)
  // Combined: number | boolean | none
```

An object with known fields unioned with a map: known fields are accessed precisely, the map branch provides a fallback for arbitrary keys. The error on unknown fields from the object branch is still reported as a diagnostic, but the type includes the map branch's result so the program is valid.

### Navigation on none

Accessing a property on `none` is not an error — it simply produces `none`. This means all property chains are implicitly optional, similar to `?.` in JavaScript:

```rex
config.routing.timeout    // if config: unknown, this is unknown | none
                          // if config: {routing: {timeout: number}}, this is number
                          // if config is none at runtime, this is none
                          // no runtime error in any case
```

If a type includes `none` in a union, navigation propagates `none`:

```rex
user = users.0            // user: {name: string} | none  (array index)
user.name                 // string | none  (none propagates)

when user do
  user.name               // string  (narrowed — user is defined)
end
```

### Type narrowing

#### Via type predicates

Rex's type predicates (`number()`, `string()`, `object()`, `array()`, `boolean()`) act as type guards:

```rex
when number(value) do
  // value: number (narrowed from whatever it was)
  value + 1    // valid
end

when string(value) do
  // value: string
  value + " suffix"    // valid
end
```

#### Via existence

The `when` construct narrows `none` out of the type:

```rex
name = req.query.name    // string | [string] | none (map lookup)

when name do
  // name: string | [string] (none removed)
end
```

#### Via comparison

Comparisons narrow the type:

```rex
when req.method == "GET" do
  // req.method: "GET" (narrowed from the union)
end
```

---

## Diagnostics

The type checker produces warnings and errors based on inferred types.

### Errors (prevent correct execution)

| Check                     | Example                                            | Message                                                       |
|---------------------------|----------------------------------------------------|---------------------------------------------------------------|
| Type mismatch in operator | `"hello" - 1`                                      | Cannot subtract from string                                   |
| Wrong argument type       | `json.parse(42)`                                   | Expected string for argument 'text' of json.parse, got number |
| Wrong argument count      | `json.parse(a, b)`                                 | json.parse expects 1 argument, got 2                          |
| Missing required field    | `{x: 1}` where `{x: number, y: number}` expected   | Missing required field 'y'                                    |
| Field type mismatch       | `{status: "ok"}` where `{status: number}` expected | Field 'status' has type string, expected number               |
| Assign to read-only       | `req.method = "POST"`                              | Cannot assign to read-only property 'method' on 'req'         |

### Warnings (likely mistakes)

| Check              | Example                                      | Message                                                |
|--------------------|----------------------------------------------|--------------------------------------------------------|
| Unknown property   | `req.headrs`                                 | Unknown property 'headrs'. Did you mean 'headers'?     |
| Unused variable    | `x = 1` (never read)                         | Variable 'x' is assigned but never used                |
| Unreachable code   | `break; x = 1`                               | Unreachable code after break                           |
| Extra object field | `{x: 1, y: 2, z: 3}` where `{x, y}` expected | Unknown field 'z' (structural subtyping allows extras) |

### Not checked

The type system intentionally does not check:

- Arithmetic overflow (numbers are arbitrary precision in Rex)
- Array bounds (out of bounds reads are none)
- Exhaustiveness of literal unions (the `else` branch handles unknown values)

There is no `any` escape hatch. `some` requires narrowing before use — the type checker ensures all operations have compatible types.

---

## How it works together

1. Developer creates `rex-serve.rexd` in the project root describing the HTTP domain
2. LSP finds and parses the `.rexd` file on startup
3. When a `.rex` file is opened, the LSP:
   - Parses the file via the rowan CST parser
   - Seeds the type environment from `.rexd` globals and any `type`/`extern` declarations in the file itself
   - Walks the CST, inferring types for each expression
   - Reports diagnostics (errors, warnings)
   - Provides completions, hover, go-to-definition from the inferred types
4. On each edit, the LSP incrementally re-parses and re-checks
