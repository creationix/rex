# Rex Type System

Rex types are inferred — no annotations required. The type checker reads declarations from `.rexd` domain files and infers everything else from literals, operators, and control flow. Types exist purely for tooling (editor diagnostics, completions, hover). The compiler and interpreter are untyped.

---

## Types

### Scalars

| Type | Description |
|------|-------------|
| `integer` | `42`, `-1`, `0` |
| `number` | Any numeric — integer or decimal (`3.14`, `1e10`) |
| `boolean` | `true` or `false` |
| `string` | `"hello"`, `'world'` |
| `null` | The null value |
| `none` | Absence — missing keys, failed lookups, deletion results |
| `"GET"` | Literal string type — only this exact value |

### Containers

| Type | Description |
|------|-------------|
| `[T]` | Array of `T` |
| `{key: T, ...}` | Object with known fields |
| `{*: T}` | Map — any string key, lookup returns `T \| none` |
| `{key: T, *: U}` | Known fields + wildcard fallback (`U \| none` for unknown keys) |

All three object forms are one internal type with optional wildcard. `{key: T}` has no wildcard (unknown keys error). `{*: T}` has no fields.

### Combinators

| Type | Description |
|------|-------------|
| `T \| U` | Union — value is `T` or `U` (narrow before use) |
| `T & U` | Intersection — value satisfies both `T` and `U` |
| `some` | Opaque defined value — must narrow before operations |
| `none` | No value — navigation on `none` produces `none` |
| `unknown` | Alias for `some \| none` |
| `never` | Unreachable — function doesn't return |
| `Name` | Reference to a `type` alias |

#### `some`, `none`, `unknown`

- **`some`** — a value exists but type is opaque. Navigation produces `some | none`. Must narrow with type predicates before arithmetic/concatenation.
- **`none`** — absence. Navigation on `none` produces `none` (no error).
- **`unknown`** — `some | none`. Might or might not exist.

```rex
extern config = some
config.timeout         // some | none

when isNumber(config.timeout) do
  config.timeout + 1   // valid — narrowed to number
end
```

#### Intersection types (`&`)

A value satisfying multiple interfaces simultaneously. Useful for host proxy values:

```rex
type HeaderValue = string & [string]
extern headers = {*: HeaderValue}

headers.host + "/path"       // valid — string operations
for v in headers.accept do   // valid — array operations
  v
end
```

Unlike unions, intersections don't require narrowing.

### Assignability

| From | To | Why |
|---|---|---|
| `integer` | `number` | Subtype |
| `"GET"` | `string` | Literal string is a string |
| any non-`none` | `some` | Any defined value |
| any type | `unknown` | `some \| none` |
| `never` | any type | Bottom type |
| `T` | `T \| U` | Member of union |

Transitive: `integer` assigns to `number`, `number` to `some`, so `integer` to `some`.

---

## Declarations

Two keywords: `type` and `extern`. Valid in `.rex` and `.rexd` files.

### `type` — named type alias

```rex
type Headers = {*: string & [string]}
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE"
```

### `extern` — host-provided binding

```rex
extern req = {method: HttpMethod, path: string, headers: Headers}
extern config = unknown
extern secrets = {*: string}
```

### `mut` — mutability

Everything is read-only by default. `mut` controls writes:

```rex
extern res = {
  mut status: integer           // field is writable
  mut headers: {mut *: string}  // field + map entries writable
  body: string                  // read-only
}
extern mut status = integer     // binding is reassignable
```

| Syntax | Meaning |
|---|---|
| `extern name = T` | Read-only binding and fields |
| `extern mut name = T` | Binding is reassignable |
| `mut field: T` | Field is writable |
| `{mut *: T}` | Map entries are writable |

### Function signatures

```rex
extern json.parse(text: string) -> some
extern json.stringify(value: some) -> string
extern log.info(message: some)
extern res.redirect(url: string, status: integer) -> never
```

Return type follows `->`. No `->` means returns `none`.

### `.rexd` files

A `.rexd` file is a `.rex` file containing only declarations. The type checker searches upward from the source file to find `*.rexd` files and loads them as the project-wide type environment.

### Documentation comments

`//` comments above a declaration become hover documentation:

```rex
// Client IP from the socket or X-Forwarded-For header
extern ip = string
```

---

## Inference

The type checker walks the program top-to-bottom, inferring a type for every expression.

### Literals

| Expression | Type |
|---|---|
| `42` | `integer` |
| `3.14` | `number` |
| `"hello"` | `string` |
| `true` | `boolean` |
| `null` | `null` |
| `none` | `none` |
| `[1, 2, 3]` | `[integer]` |
| `{a: 1, b: "x"}` | `{a: integer, b: string}` |

### Variables

```rex
x = 42                                    // x: integer
lookup: {*: integer} = {a: 1, b: 2}      // explicit type annotation
```

Type annotations use `name: Type = value`. The value must be assignable to the declared type. Without an annotation, the type is inferred from the value.

### Operators

**Arithmetic** — `+` is strict. `string + number` is a type error. Use template literals for coercion.

| Expression | Type |
|---|---|
| `integer + integer` | `integer` |
| `number + number` | `number` |
| `string + string` | `string` |
| `number + string` | error |
| `some + T` | error (narrow first) |

**Comparison** — returns `typeof(lhs) | none`:

```rex
x > 10       // number | none
x == "GET"   // typeof(x) | none
```

**Logical** — existence-based, `and` binds tighter than `or`:

| Expression | Type |
|---|---|
| `a or b` | `typeof(a) \| typeof(b)` |
| `a and b` | `typeof(b) \| none` |

### String coercion

Only in template literals. The `+` operator does not coerce.

```rex
`count: ${x}`     // valid — any type coerced to string
"count: " + x     // error if x is not string
```

### Property access

```rex
req.method         // HttpMethod (known field)
req.headers.host   // string & [string] | none (map lookup)
point.z            // warning: unknown property 'z'. Did you mean 'x'?
```

Navigation on `none` produces `none` (no error). Navigation on `some` produces `some | none`.

### Control flow

```rex
when cond do body end          // typeof(body) | none
when cond do a else b end      // typeof(a) | typeof(b)
return expr                    // never
for x in items do body end     // typeof(body) | none
[x * 2 for x in items]        // [typeof(x * 2)]
```

---

## Narrowing

Type narrowing refines a variable's type inside a branch.

### Existence

```rex
when name do
  // name: none removed from type
end
```

### Type predicates

```rex
when isNumber(value) do
  value + 1    // value: number
end
```

### Comparison

```rex
when method == "GET" do
  // method: "GET"
end
```

### `and` chains

```rex
when input and input.slug do
  // input: defined, input.slug: defined
end
```

### Flow-sensitive

After `unless cond do return end`, subsequent code knows `cond` is true:

```rex
unless auth do
  return {error: "unauthorized"}
end
// auth is defined here
```

---

## Diagnostics

### Errors

| Check | Example | Message |
|---|---|---|
| Type mismatch | `"hello" - 1` | Cannot subtract from string |
| Wrong arg type | `json.parse(42)` | Expected string, got integer |
| Wrong arg count | `json.parse(a, b)` | Expects 1 argument, got 2 |
| Assign to read-only | `req.method = "POST"` | Cannot assign to read-only property |

### Warnings

| Check | Example | Message |
|---|---|---|
| Unknown property | `req.headrs` | Did you mean 'headers'? |
| Unused variable | `x = 1` (never read) | Variable 'x' is assigned but never used |

### Not checked

- Arithmetic overflow
- Array bounds
- Exhaustiveness of unions

No `any` escape hatch. `some` must be narrowed.
