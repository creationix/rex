# Rex High-Level Syntax

An infix syntax for Rex that compiles to the same bytecode as the [core s-expression language](core-language.md). Designed for users who want to configure a system and learn as little new stuff as possible.

## Guiding Principles

Rex uses **existence** instead of truthiness. There is no concept of "falsy" — `false`, `null`, `0`, and `""` are all ordinary values. Only `undefined` represents absence. Control flow checks whether something is defined, not whether it's "truthy."

This single idea drives the entire language:

- **Comparisons** return the left-hand value on success, `undefined` on failure
- **`when`/`unless`** branch on whether a value is defined
- **`and`/`or`** short-circuit on existence, not truthiness
- **Type predicates** return the value if it matches, `undefined` otherwise

Because there's no truthiness, there are no truthiness bugs:

```rex
0 or "fallback"        // → 0 (zero is a value, not absence)
false or "fallback"    // → false (false is a value, not absence)
null or "fallback"     // → null (null is a value, not absence)
undefined or "fallback" // → "fallback" (undefined IS absence)
```

## Data Types

Same as the core language. Rex is a superset of JSON with a few additions:

```rex
// JSON types
42, -3.14, 1e10               // numbers
"hello", 'world'              // strings
"\"\\\/\b\f\n\r\t\u1234"      // JSON escape codes
true, false                   // booleans
null                          // null
[1, 2, 3]                     // arrays
{"name": "Rex", "age": 65}    // objects

// Rex additions
undefined                     // absence of value
0xFF, 0b1010                  // hex and binary numbers
"null\0byte"                  // \0 null byte escape
"\x48\x65\x6c\x6c\x6f"        // \xHH hex byte escapes
'single quoted strings'       // single quotes allowed
'escaped \' single'           // escaped single quotes
[1 2 3]   {a:1 b:2}           // commas are optional
[1,2,3,]  {a:1,b:2,}          // trailing comma allowed
{name: "Rex", age: 65}        // bare string keys allowed when [0-9a-zA-Z_-]+
{404:"Not Found" 500:"Error"} // integer keys allowed (stay as integers)
```

## Comments

```rex
// Line comment

/* Block comment */

/* Multi-line
   block comment */
```

## Navigation

Dots navigate into nested structures:

```rex
user.name                  // read a key
user.address.street        // nested navigation
headers.content-type       // domain built-in navigation
self.email                 // navigate into implicit value
```

For dynamic keys (when the key is a variable or expression), use `.()`:

```rex
map.(key)                  // key is a variable
table.(x + 1)              // key is a computed expression
config.(headers.x-action)  // key is a dotted path

// Chaining
foo.bar.(key).baz          // static, dynamic, static
table.(k1).(k2)            // multiple dynamic keys
```

Bare `()` is always grouping — `.()` is always navigation. No ambiguity, no significant whitespace:

```rex
foo (a + b)                // two expressions: foo, then (a + b)
foo.(a + b)                // one expression: look up (a + b) in foo
```

## Assignment

`=` binds a value to a place and returns the value:

```rex
x = 42
obj.key = "value"
headers.x-handler = self
```

Compound assignment operators modify a place using an operator and return the new value:

```rex
x += 1           // x = x + 1
x -= 5           // x = x - 5
x *= 2           // x = x * 2
x /= 10          // x = x / 10
x %= 3           // x = x % 3
x &= 0xFF        // x = x & 0xFF
x |= 0x80        // x = x | 0x80
x ^= mask        // x = x ^ mask
```

## Operators

### Arithmetic

```rex
x + y          // add
x - y          // subtract
x * y          // multiply
x / y          // divide
x % y          // modulo
-x             // negate
```

### Comparison

All comparisons return the left-hand value on success, `undefined` on failure:

```rex
age > 18       // → age if true, undefined if false
age >= 18      // → age if true, undefined if false
age < 65       // → age if true, undefined if false
age <= 65      // → age if true, undefined if false
x == y         // → x if equal, undefined if not
x != y         // → x if not equal, undefined if equal
```

This means comparisons compose directly with `when` and `and`:

```rex
when age > 18 do
  allow(self)
end

when age > 18 and age < 65 do
  process(self)
end
```

### Existence Operators

`and` and `or` short-circuit based on existence (defined vs `undefined`). They return actual values, not booleans:

| Operator  | Returns                                          |
|-----------|--------------------------------------------------|
| `a and b` | `b` if both defined, first `undefined` otherwise |
| `a or b`  | first defined value, `undefined` otherwise       |

```rex
// or — first defined value (nullish coalescing)
user.preferred-name or user.name or "anonymous"

// and — last value if all defined
user and user.name and user.email
```

These are variadic in the bytecode, but in infix they chain naturally with standard left-to-right evaluation.

If you want a ternary expression `a ? b : c`, you can use `a and b or c` if b is known to be defined, otherwise use `when a do b else c end`.

### Bitwise / Boolean Value Operators

Symbol operators that work on the values themselves. On booleans they perform boolean algebra, on numbers they perform bitwise operations:

| Operator | Booleans    | Numbers     |
|----------|-------------|-------------|
| `a & b`  | Boolean AND | Bitwise AND |
| `a \| b` | Boolean OR  | Bitwise OR  |
| `a ^ b`  | Boolean XOR | Bitwise XOR |
| `~a`     | Boolean NOT | Bitwise NOT |

```rex
// Boolean algebra
true & false               // false
true | false               // true

// Bitwise operations
0xFF & 0x0F                // 15
0x0F | 0xF0                // 255
0xFF ^ 0x0F                // 240
~0x0F                      // bitwise NOT

// Computing with boolean data
user.can-edit = user.is-admin & ~user.is-suspended
```

The distinction: **words** (`and`, `or`) operate on **existence**. **Symbols** (`&`, `|`, `^`, `~`) operate on **values**.

### Operator Precedence

Highest to lowest:

| Level | Operators                   | Category                |
|-------|-----------------------------|-------------------------|
| 1     | `.` `.()`                   | navigation              |
| 2     | `-x` `~x`                   | unary                   |
| 3     | `*` `/` `%`                 | multiplicative          |
| 4     | `+` `-`                     | additive                |
| 5     | `&` `^` `\|`                | bitwise / boolean value |
| 6     | `==` `!=` `>` `>=` `<` `<=` | comparison              |
| 7     | `and` `or`                  | existence               |
| 8     | `=` `+=` `-=` `*=` `/=` `%=` `&=` `\|=` `^=` | assignment |

Use `()` to override:

```rex
(a + b) * c
a * (b + c)
```

## Control Flow

### `when` / `unless`

`when` runs its body if the condition is defined (not `undefined`). `unless` is the inverse.

```rex
when age > 18 do
  allow(self)
end

unless string(value) do
  handle-non-string()
end
```

With else:

```rex
when authorized do
  proceed()
else
  deny()
end
```

Chain with `else when`:

```rex
when string(value) do
  handle-string(self)
else when number(value) do
  handle-number(self)
else
  handle-other()
end
```

### Binding in Conditions

`=` inside a `when` condition both binds the value and checks existence. `self` is also set to the condition value:

```rex
when x = get-data() do
  use(x)            // x is the value
  use(self)         // self is also the value
end
```

Named bindings are aliases for `self` — useful to avoid shadowing in nested scopes:

```rex
when x = get-primary() do
  use(x)
else when y = get-fallback() do
  use(y)
else
  use-default()
end
```

## Iteration

### `for` Loops

`for` iterates over a value and executes a body for each element. Returns the last expression of the last iteration.

```rex
// Implicit — self is each value
for [1, 2, 3] do
  process(self)
end

// Named value — self is also set to v
for v in [1, 2, 3] do
  process(v)
end

// Key and value
for k, v in [1, 2, 3] do
  // k = 0, 1, 2  v = 1, 2, 3
  process(k, v)
end

// Keys only — self is the key
for k of {a: 1, b: 2, c: 3} do
  // k = "a", "b", "c"
  log(k)
end
```

In `in` forms, `self` is set to the current value. In `of` forms, `self` is set to the current key.

### Iterable Types

`for` works on arrays, objects, strings, and numbers:

| Input | Keys (k) | Values (v) |
|---|---|---|
| `[10, 20, 30]` | `0, 1, 2` | `10, 20, 30` |
| `{a: 1, b: 2}` | `"a", "b"` | `1, 2` |
| `"Hello"` | `0, 1, 2, 3, 4` | `"H", "e", "l", "l", "o"` |
| `5` | `0, 1, 2, 3, 4` | `1, 2, 3, 4, 5` |

Number values are 1-based (counting to N). Keys are always 0-based.

Domain extensions can add new iterable types — for example, URL paths that iterate over segments or domain names that iterate over subdomains.

### `break` and `continue`

`break` exits the loop early. `continue` skips to the next iteration:

```rex
for v in [1, 2, 3, 4, 5] do
  when v == 4 do break end
  when v % 2 != 0 do continue end
  process(v)    // processes 2 only
end
```

### Comprehensions

Comprehensions build new collections using `;` to separate the iteration clause from the body expression. The iteration clause uses the same `in`/`of` forms as `for`, or just an expression for implicit `self`.

#### Array Comprehensions

```rex
// Implicit self
[100 ; self % 2 > 0 and self % 3 > 0 and self % 5 > 0]
// → [1, 7, 11, 13, 17, 19, 23, 29, 31, ...]

// Named value
[v in [1, 2, 3] ; v * 2]
// → [2, 4, 6]

// Key and value
[k, v in [10, 20, 30] ; v + k]
// → [10, 21, 32]

// Keys only
[k of {name: "Rex", age: 65} ; k]
// → ["name", "age"]
```

#### Object Comprehensions

Object comprehensions use `key-expr: value-expr` after `;`. Both sides are expressions — bare words reference variables, not literal strings (use quotes for literal keys):

```rex
{k, v in {a: 1, b: 2} ; k: v * 10}
// → {a: 10, b: 20}

{v in ["x", "y", "z"] ; v: true}
// → {x: true, y: true, z: true}

{k, v in scores ; "player-" + k: v * 100}
// → {"player-alice": 9500, "player-bob": 8700}

// Implicit self
{users ; self.name: self.score}
// → {Alice: 95, Bob: 87}
```

#### Filtering

Return `undefined` to exclude an element from the result:

```rex
// Even numbers only
[v in [1, 2, 3, 4, 5] ; v % 2 == 0 and v]
// → [2, 4]

// Remove null values from an object
{k, v in data ; k: v != null and v}
// → new object without null values
```

## Type Predicates

Type predicates return the value if it matches the type, `undefined` otherwise. They use keyword call syntax:

| Predicate       | Returns                             |
|-----------------|-------------------------------------|
| `string(expr)`  | `expr` if string, else `undefined`  |
| `number(expr)`  | `expr` if number, else `undefined`  |
| `object(expr)`  | `expr` if object, else `undefined`  |
| `array(expr)`   | `expr` if array, else `undefined`   |
| `boolean(expr)` | `expr` if boolean, else `undefined` |

Because they return value-or-undefined, they compose with `when`:

```rex
when n = number(value) do
  n + 1
end

// Type dispatch
when string(value) do
  handle-string(self)
else when number(value) do
  handle-number(self)
else
  handle-other()
end
```

## Delete

Removes a key from a place:

```rex
delete obj.key
delete user.temp
```

## Self

`self` refers to the implicit value in the current scope — the condition in `when`/`unless`, or the current element in `for` loops and comprehensions:

```rex
when get-data() do
  process(self)        // self is the result of get-data()
  self.name            // navigate into it
end
```

`self` is navigable like any other place:

```rex
self.name
self.email
self.address.street
```

## Worked Examples

### Nullish Coalescing

```rex
// S-expression equivalent: (alt user.preferred-name user.name "anonymous")
user.preferred-name or user.name or "anonymous"
```

### Range Check

```rex
// S-expression equivalent: (when (all (gt age 18) (lt age 65)) (process self))
when age > 18 and age < 65 do
  process(self)
end
```

### Object Lookup with Fallback

```rex
// S-expression equivalent: (when (map headers.x-action) (set headers.x-handler self))
actions = {
  abc: "/letters"
  123: "/numbers"
}
when actions.(headers.x-action) do
  headers.x-handler = self
end
```

Or inline:

```rex
when {
  abc: "/letters"
  123: "/numbers"
}.(headers.x-action) do
  headers.x-handler = self
end
```

### Type-Safe Processing

```rex
when n = number(input) do
  total = total + n
else when s = string(input) do
  log("got string: " + s)
else
  log("unexpected type")
end
```

### Boolean Data with Existence Logic

```rex
// Compute a boolean value (value operators)
user.can-edit = user.is-admin & ~user.is-suspended

// Then branch on whether it's defined AND true
when user.can-edit do
  show-editor()
end
```

### Chained Fallbacks

```rex
when x = get-primary() do
  use(x)
else when y = get-fallback() do
  use(y)
else
  use-default()
end
```

### Accumulating Values

```rex
total = 0
for [10, 20, 30] do
  total += self
end
// total is 60
```

### Building a Lookup Table

```rex
users = [{name: "Alice", id: 1}, {name: "Bob", id: 2}]
lookup = {v in users ; v.name: v}
// → {Alice: {name: "Alice", id: 1}, Bob: {name: "Bob", id: 2}}
```

### Filtering and Transforming

```rex
scores = {alice: 95, bob: 42, carol: 78}

// Students who passed (score >= 50)
passed = {k, v in scores ; k: v >= 50 and v}
// → {alice: 95, carol: 78}

// Just the names
passed-names = [k, v in scores ; v >= 50 and k]
// → ["alice", "carol"]
```

### Side-by-Side: HTTP API Router

A realistic example using the HTTP routing domain extension. S-expression first, then infix:

**S-expression:**

```rex
(when (path-match "/api/users/*")
  (when (eq method "GET")
    (do
      status = 200
      headers.content-type = "application/json")
    (when (eq method "POST")
      (when (all
              (string headers.content-type)
              (eq headers.content-type "application/json"))
        (do
          status = 201
          headers.x-created = self.id)
        (do
          status = 415
          headers.x-error = "Expected application/json"))
      (do
        status = 405
        headers.x-error = "Method not allowed"))))
```

**Infix:**

```rex
when path-match("/api/users/*") do
  when method == "GET" do
    status = 200
    headers.content-type = "application/json"
  else when method == "POST" do
    when string(headers.content-type) and headers.content-type == "application/json" do
      status = 201
      headers.x-created = self.id
    else
      status = 415
      headers.x-error = "Expected application/json"
    end
  else
    status = 405
    headers.x-error = "Method not allowed"
  end
end
```

### Side-by-Side: Access Control

**S-expression:**

```rex
(when (all
        headers.x-api-key
        (eq headers.x-api-key config.api-key))
  (when (eq method "GET")
    (set headers.x-allowed "true")
    (when (all (eq method "POST") user.is-admin)
      (set headers.x-allowed "true")
      (do
        status = 403
        headers.x-error = "Forbidden")))
  (do
    status = 401
    headers.x-error = "Invalid API key"))
```

**Infix:**

```rex
when headers.x-api-key and headers.x-api-key == config.api-key do
  when method == "GET" do
    headers.x-allowed = "true"
  else when method == "POST" and user.is-admin do
    headers.x-allowed = "true"
  else
    status = 403
    headers.x-error = "Forbidden"
  end
else
  status = 401
  headers.x-error = "Invalid API key"
end
```

### Side-by-Side: Request Transformation

**S-expression:**

```rex
(when (path-match "/api/search")
  (do
    query-term = (alt query.q query.query query.search)
    (when query-term
      (do
        results = (search-index query-term)
        headers.x-result-count = results.count
        (when (gt results.count 0)
          (do
            status = 200
            headers.content-type = "application/json")
          (do
            status = 404
            headers.x-error = "No results")))
      (do
        status = 400
        headers.x-error = "Missing search query"))))
```

**Infix:**

```rex
when path-match("/api/search") do
  query-term = query.q or query.query or query.search

  when query-term do
    results = search-index(query-term)
    headers.x-result-count = results.count

    when results.count > 0 do
      status = 200
      headers.content-type = "application/json"
    else
      status = 404
      headers.x-error = "No results"
    end
  else
    status = 400
    headers.x-error = "Missing search query"
  end
end
```

## Keyword Reference

### Mapping from Core Language

The high-level infix syntax maps to the same bytecode as the s-expression core:

| Core (s-expression) | High-level (infix)         |
|---------------------|----------------------------|
| `(add a b)`         | `a + b`                    |
| `(sub a b)`         | `a - b`                    |
| `(mul a b)`         | `a * b`                    |
| `(div a b)`         | `a / b`                    |
| `(mod a b)`         | `a % b`                    |
| `(neg a)`           | `-a`                       |
| `(eq a b)`          | `a == b`                   |
| `(neq a b)`         | `a != b`                   |
| `(gt a b)`          | `a > b`                    |
| `(gte a b)`         | `a >= b`                   |
| `(lt a b)`          | `a < b`                    |
| `(lte a b)`         | `a <= b`                   |
| `(and a b)`         | `a & b`                    |
| `(or a b)`          | `a \| b`                   |
| `(not a)`           | `~a`                       |
| `(xor a b)`         | `a ^ b`                    |
| `(all a b ...)`     | `a and b and ...`          |
| `(alt a b ...)`     | `a or b or ...`            |
| `(when c t e)`      | `when c do t else e end`   |
| `(unless c t e)`    | `unless c do t else e end` |
| `(set x v)`         | `x = v`                    |
| `(delete x)`        | `delete x`                 |
| `(actions key)`     | `actions.(key)`            |
| `actions.name`      | `actions.name`             |
| `(string v)`        | `string(v)`                |
| `self`              | `self`                     |

## Reserved Words

**Literals:** `true`, `false`, `null`, `undefined`, `self`

**Control flow:** `when`, `unless`, `for`, `in`, `of`, `do`, `else`, `end`, `break`, `continue`, `and`, `or`

**Place operations:** `delete`

**Type predicates:** `string`, `number`, `object`, `array`, `boolean`

## Extension Points

Domain extensions work the same as in the core language. Extensions register navigable places and opcodes that the compiler recognizes as keywords. For example, an HTTP routing domain might add:

```rex
// Domain places — navigable with dot and .()
headers.content-type
query.page
cookies.session-id

// Domain opcodes — keyword call syntax
path-match("/api/*")
domain-match("*.example.com")

// Mutations — assignment to domain places
headers.x-handler = self
status = 200
method = "POST"
```
