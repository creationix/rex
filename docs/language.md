# Rex Language Reference

Rex is a small expression language for configuring systems. It's a superset of JSON with variables, control flow, and existence-based logic.

## Core Idea: Existence, Not Truthiness

Rex has no concept of "falsy." `false`, `null`, `0`, and `""` are ordinary values. Only `none` represents absence.

```rex
0 or "fallback"       // 0 — zero is a value
false or "fallback"   // false — false is a value
null or "fallback"    // null — null is a value
none or "fallback"    // "fallback" — none IS absence
```

This drives everything: comparisons return values or `none`, `when` branches on existence, `and`/`or` short-circuit on existence.

---

## Data

Rex is a superset of JSON:

```rex
42                      // integers
3.14                    // decimals
0xFF                    // hex
0b1010                  // binary
"hello"                 // double-quoted strings
'world'                 // single-quoted strings
true false              // booleans
null                    // null
none                    // absence
inf                     // infinity
nan                     // not a number
[1, 2, 3]              // arrays (commas optional)
{name: "Rex", age: 65} // objects (bare keys, commas optional, trailing allowed)
```

### Computed Keys

Bare keys are literal strings. Parentheses make them expressions:

```rex
{name: "Rex"}          // key is "name"
{(name): "Rex"}        // key is the value of variable `name`
{(x + 1): "value"}     // any expression
```

### Template Literals

Backtick strings with `${expr}` interpolation:

```rex
`hello ${name}, you have ${count} items`
`no escaping "needed" here`
```

Tagged templates pass string parts and values to a function:

```rex
html`<p>${user-input}</p>`    // html receives (["<p>", "</p>"], user-input)
```

Tags enable safe patterns — an `html` tag auto-escapes interpolated values to prevent XSS.

### Comments

```rex
// line comment
/* block comment */
```

---

## Expressions

### Navigation

Dots read nested values. `.()` navigates with dynamic keys:

```rex
user.name              // static key
user.address.street    // nested
map.(key)              // dynamic key (variable)
table.(x + 1)          // dynamic key (expression)
```

`foo (a)` is two expressions. `foo.(a)` is navigation.

### Assignment

```rex
x = 42                 // bind and return value
x: {*: integer} = {}   // with type annotation (for type checker)
old = x := 2           // swap: returns previous value
x += 1                 // compound: +=  -=  *=  /=  %=  &=  |=  ^=
```

### Arithmetic

```rex
x + y    x - y    x * y    x / y    x % y    -x
```

### Comparison

Returns the left-hand value on success, `none` on failure:

```rex
age > 18     // age if true, none if false
x == y       // x if equal, none if not
```

Composes naturally with `when`:

```rex
when age > 18 and age < 65 do
  process(age)
end
```

### Bitwise / Boolean

**Symbols** (`&`, `|`, `^`, `~`) operate on values. **Words** (`and`, `or`) operate on existence.

| Operator | Booleans | Numbers |
|---|---|---|
| `&` | AND | Bitwise AND |
| `\|` | OR | Bitwise OR |
| `^` | XOR | Bitwise XOR |
| `~` | NOT | Bitwise NOT |

### Logic (Existence)

`and` and `or` short-circuit on existence, not truthiness. `and` binds tighter than `or`.

| Expression | Returns |
|---|---|
| `a or b` | First defined value |
| `a and b` | `b` if both defined, first `none` otherwise |

```rex
name or "anonymous"          // nullish coalescing
user and user.email          // guard chain
a and b or c                 // (a and b) or c
```

### Compound Expressions

Semicolons group multiple expressions into a single expression, like C's comma operator. Evaluates left to right, returns the last value:

```rex
a = 1; b = 2; a + b       // 3

// Useful in single-expression positions:
when setup(); ready do go() end
[ c = a + b; a = b; b = c while a <= 100 ]
```

Semicolons force expression boundaries: `a; -b` is two expressions (`a` then negate `b`), while `a - b` is one (subtraction).

### Operator Precedence

Highest to lowest:

| Level | Operators | Category |
|---|---|---|
| 1 | `.` `.()` | navigation |
| 2 | `-x` `~x` | unary |
| 3 | `*` `/` `%` | multiplicative |
| 4 | `+` `-` | additive |
| 5 | `..` | range |
| 6 | `==` `!=` `>` `>=` `<` `<=` | comparison |
| 7 | `&` `^` `\|` | bitwise / boolean |
| 8 | `and` | existence and |
| 9 | `or` | existence or |
| 10 | `=` `:=` `+=` etc. | assignment |
| 11 | `;` | compound expression |

### Type Predicates

Return the value if it matches, `none` otherwise:

```rex
isString(x)    isNumber(x)    isInteger(x)    isBoolean(x)    isArray(x)    isObject(x)
```

Compose with `when` for type dispatch:

```rex
when n = isNumber(value) do
  n + 1
else when s = isString(value) do
  s + " suffix"
end
```

---

## Control Flow

### `when` / `unless`

Branch on existence:

```rex
when age > 18 do allow(age) end

unless authorized do deny() end

when method == "GET" do
  handle-get()
else when method == "POST" do
  handle-post()
else
  {error: "method not allowed"}
end
```

### Binding in Conditions

`=` in a condition binds the value and checks existence:

```rex
when data = get-data() do
  use(data)
end
```

### `return`

Halts execution and produces a value:

```rex
unless auth do
  return {error: "unauthorized"}
end
// auth is defined here
```

### `delete`

Removes a key:

```rex
delete obj.key
```

---

## Iteration

### `for` Loops

```rex
for v in [1, 2, 3] do process(v) end        // values
for k, v in {a: 1, b: 2} do log(k, v) end   // key + value
for k of {a: 1, b: 2} do log(k) end         // keys only
```

### `while` Loops

```rex
while x < 10 do
  x += 1
end
```

### Ranges

```rex
1..5       // [1, 2, 3, 4, 5] — inclusive
5..1       // [5, 4, 3, 2, 1] — auto-descending
```

### `break` / `continue`

```rex
for v in items do
  when v == 4 do break end
  process(v)
end
```

### Comprehensions

Body first, then iteration:

```rex
[v * 2 for v in [1, 2, 3]]                    // [2, 4, 6]
[v % 2 == 0 and v for v in 1..10]             // [2, 4, 6, 8, 10] — filter
{(k): v * 10 for k, v in {a: 1, b: 2}}       // {a: 10, b: 20}
```

Return `none` to exclude an element.

`while` comprehensions collect values until the condition fails:

```rex
x = 1
[x = x * 2 while x < 100]                    // [2, 4, 8, 16, 32, 64, 128]
```

The body can have multiple expressions — all are evaluated per iteration, the last is collected:

```rex
a = 0; b = 1
[c = a + b
  a = b
  b = c
  while a <= 100]                              // fibonacci: [1, 2, 3, 5, ...]
```

### Iterable Types

| Input | Keys | Values |
|---|---|---|
| `[10, 20]` | `0, 1` | `10, 20` |
| `{a: 1}` | `"a"` | `1` |
| `"Hi"` | `0, 1` | `"H", "i"` |

Iteration is deterministic. Snapshot semantics: mutations during iteration don't affect the current pass.

---

## Declarations

See [rex-types.md](rex-types.md) for full type system documentation.

### `type` — named type alias

```rex
type Headers = {*: string & [string]}
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE"
```

### `extern` — host-provided binding

```rex
extern req = {method: HttpMethod, path: string, headers: Headers}
extern res = {mut status: integer, mut headers: {mut *: string}}
extern json.parse(text: string) -> some
extern log.info(message: some)
```

`mut` controls writability per field. `->` specifies return type. Everything is read-only by default.

---

## Reserved Words

**Literals:** `true`, `false`, `null`, `none`, `inf`, `nan`

**Control flow:** `when`, `unless`, `for`, `in`, `of`, `do`, `else`, `end`, `break`, `continue`, `and`, `or`, `return`, `while`

**Operations:** `delete`

**Declarations:** `type`, `extern`

---

## Examples

### Nullish Coalescing

```rex
user.preferred-name or user.name or "anonymous"
```

### Lookup Table

```rex
users = [{name: "Alice", id: 1}, {name: "Bob", id: 2}]
lookup = {(v.name): v for v in users}
```

### Filtering

```rex
scores = {alice: 95, bob: 42, carol: 78}
passed = {(k): v >= 50 and v for k, v in scores}
// {alice: 95, carol: 78}
```

### HTTP Router

```rex
when method == "GET" do
  return {ok: true, data: items}
end
when method == "POST" do
  return {ok: true, created: id}
end
res.status = 405
{ok: false, error: "method not allowed"}
```

### Access Control

```rex
unless headers.x-api-key and headers.x-api-key == config.api-key do
  res.status = 401
  return {error: "invalid API key"}
end

when method == "POST" and user.is-admin do
  proceed()
else
  res.status = 403
  {error: "forbidden"}
end
```
