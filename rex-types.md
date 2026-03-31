# Rex Type System

Rex has no user-space type annotations. Types are inferred from domain interface files (`.rexd`), literals, operators, and type predicates. The type system exists purely for tooling — the compiler and interpreter are untyped.

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
| `{key: T, ...}` | Object with known fields |
| `{*: T}` | Map — any string key, lookup returns `T \| none` |
| `{key: T, *: U}` | Object with known fields and wildcard fallback |

### Meta types

| Type | Description |
|------|-------------|
| `some` | A value exists but its type is opaque — must narrow before use |
| `T \| U` | Union — value can be `T` or `U` |
| `unknown` | Alias for `some \| none` |
| `never` | Function does not return (throws or diverges) |
| `Name` | Reference to a type alias defined in `.rexd` |

---

## Domain Interface Files (`.rexd`)

A `.rexd` file declares the types of globals, functions, and type aliases available to Rex programs in a given project. The LSP searches upward from the open file for `*.rexd` files.

### Syntax

`.rexd` files use Rex-like syntax for familiarity but describe types, not executable code.

#### Type aliases

Uppercase names define reusable types:

```rex
Headers = {*: string | [string]}
HttpMethod = "GET" | "POST" | "PUT" | "DELETE" | "PATCH"
```

#### Globals

Lowercase names declare values available to Rex programs:

```rex
// Read-only (default)
req = {
  method: HttpMethod
  path: string | [string]
  headers: Headers
  query: {*: string | [string]}
  cookies: {*: string}
  ip: string
  body: string
}

// Mutable — the program can assign to properties
mut res = {
  status: number
  headers: Headers
  body: string
}

// Simple typed globals
config: unknown
secrets: {*: string}
```

#### Functions

Dot-path names with typed arguments and optional return type:

```rex
log.info(message: unknown)
log.warning(message: unknown)
json.parse(text: string): unknown
json.stringify(value: unknown): string
res.rewrite(url: string): never
res.redirect(url: string, status: number): never
```

#### Comments as documentation

`//` comments immediately above a declaration are extracted as hover documentation:

```rex
// Client IP address from the connecting socket or X-Forwarded-For header
ip: string
```

### Type syntax

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
| `{*: T}`        | Map — any string key accepted, lookup returns `T \| none` (key may not exist) |
| `{key: T, *: U}` | Object with known fields (exact type) and a wildcard fallback (`U \| none`) for other keys |
| `T \| U`        | Union — value can be `T` or `U`                     |
| `Name`          | Reference to a type alias                           |

### `some`, `none`, and `unknown`

Three primitive concepts for presence and absence:

- **`some`** — a value exists, but we don't know its type. Must narrow with type predicates before using in operations. Navigation on `some` produces `some | none` (the key might not exist).
- **`none`** — no value. The result of missing properties, failed comparisons, deletion tombstones. Navigation on `none` produces `none`.
- **`unknown`** — alias for `some | none`. Might be a value, might be absent. Common for map lookups and opaque domain fields.

```rex
config: some                         // definitely a value, type opaque
config.timeout                       // some | none (key may not exist)

cookies.session                      // string | none (map lookup)

when cookies.session do
  // cookies.session: string (none removed — value exists)
end

data: unknown                        // same as some | none
when data do
  // data: some (none removed — value exists)
  when number(data) do
    // data: number (narrowed further)
    data + 1                         // valid
  end
end
```

### Mutability

Globals are read-only by default. Use `mut` to allow property writes:

```rex
mut res = { status: number, headers: Headers, body: string }
```

Inside Rex code, `res.status = 404` is valid. Without `mut`, the LSP reports an error on write attempts.

### No short codes

`.rexd` files describe the developer-facing API. The names match what the programmer writes in Rex code (`req.headers`, `res.status`). The compiler maps these to internal short ref codes (`'H`, `'S`) via a separate host-provided mapping. The developer never sees or writes short codes.

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

When a non-string value appears in a string context (template literals, `+` with a string operand), it is coerced to a string representation:

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

This only applies to string coercion contexts. JSON serialization, comparison, and type checking use the actual values.

### Arithmetic operators

| Expression                         | Type                                                                         |
|------------------------------------|------------------------------------------------------------------------------|
| `integer + integer`                | `integer`                                                                    |
| `number + number`                  | `number`                                                                     |
| `string + string`                  | `string` (concatenation)                                                     |
| `number + string`                  | error: cannot add number and string                                          |
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

Rex uses existence-based logic, not boolean logic:

| Expression | Type                     | Semantics                          |
|------------|--------------------------|------------------------------------|
| `a or b`   | `typeof(a) \| typeof(b)` | First defined value                |
| `a and b`  | `typeof(b) \| none` | Second value if first is defined   |
| `a nor b`  | `typeof(b) \| none` | Second value if first is none |

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

**Loops:**

```rex
for x in items do body end      // type: typeof(body) (last iteration)
while cond do body end          // type: typeof(body) | none
```

**Comprehensions produce arrays:**

```rex
[x * 2 for x in items]         // type: [number] (if items: [number])
[self in 1..10]                 // type: [integer]
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

This means a union of a known-field object and a map is useful: known fields are accessed precisely, but the map branch provides a fallback for arbitrary keys.

```rex
// A config that has known fields but also allows extensions
Config = {timeout: number, retries: integer} | {*: unknown}

config.timeout    // number | unknown | none
config.custom     // error on left branch (unknown field), unknown | none on right
                  // combined: unknown | none
```

An object with known fields unioned with a map does NOT suppress the error on unknown fields from the object branch — the error is still reported as a diagnostic, but the type includes the map branch's result so the program is valid.

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

This means the type checker never reports a navigation chain as an error. Unknown properties on concrete types produce a warning and type `none`, but the program is still valid.

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

1. Developer creates `http.rexd` in the project root describing the HTTP domain
2. LSP finds and parses the `.rexd` file on startup
3. When a `.rex` file is opened, the LSP:
   - Parses the file via the rowan CST parser
   - Seeds the type environment from the `.rexd` globals
   - Walks the CST, inferring types for each expression
   - Reports diagnostics (errors, warnings)
   - Provides completions, hover, go-to-definition from the inferred types
4. On each edit, the LSP incrementally re-parses and re-checks
5. No type annotations appear in `.rex` files — everything is inferred
