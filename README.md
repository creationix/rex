# 🦖 Rex

<picture align="right">
  <source media="(prefers-color-scheme: dark)" srcset="img/rex-mascot-dark.png">
  <source media="(prefers-color-scheme: light)" srcset="img/rex-mascot-light.png">
  <img alt="Rex mascot" src="img/rex-mascot-light.png" align="right" width="200">
</picture>

Programmable JSON. Small arms, big bite.

Rex is a compact expression language for configuration and data-driven logic. It is a superset of JSON with high-level syntax (`when`, `unless`, `and`, `or`, assignment, loops, comprehensions) that compiles to a compact bytecode string format called `rexc`.

If you need data-first configs with just enough logic — without embedding a full scripting runtime — Rex is built for that.

## Why Rex

When JSON-only configs hit real-world logic, teams usually end up with one of these:

- Massive duplicated rules
- Embedded JavaScript/Lua/WASM runtimes
- A custom DSL that keeps growing forever

Rex keeps the model data-oriented and explicit:

- JSON-like literals and object/array ergonomics
- Existence-based control flow (`undefined` means absence)
- Compact, serializable `rexc` bytecode output
- Domain-agnostic core language

## Existence Semantics

Rex uses **existence** instead of truthiness. There is no concept of "falsy" — `false`, `null`, `0`, and `""` are all ordinary values. Only `undefined` represents absence:

```rex
0 or "fallback"         // => 0         (zero is a value)
false or "fallback"     // => false     (false is a value)
null or "fallback"      // => null      (null is a value)
undefined or "fallback" // => "fallback" (undefined IS absence)
```

This single idea drives the entire language:

- **Comparisons** return the left-hand value on success, `undefined` on failure
- **`when`/`unless`** branch on whether a value is defined
- **`and`/`or`/`nor`** short-circuit on existence, not truthiness
- **Type predicates** return the value if it matches, `undefined` otherwise

Because there's no truthiness, there are no truthiness bugs.

## Quick Example

Table-driven routing — look up an action in a map and set a header:

```rex
actions = {
  create-user: "users/create"
  delete-user: "users/delete"
  update-profile: "users/update-profile"
}

when handler = actions.(headers.x-action) do
  headers.x-handler = handler
end
```

This compiles to a single `rexc` bytecode string:

```rexc
(%=actions$1p{create-user:c,users/createdelete-user:c,users/deleteupdate-profile:k,users/update-profile}?(=handler$r(actions$(headers$x-action:))s=(headers$x-handler:)handler$))
```

## Language Overview

Rex is a superset of JSON. Every valid JSON document is already valid Rex.

### Data Types

```rex
// Everything from JSON
42   -3.14   "hello"   true   false   null
[1, 2, 3]   {"name": "Rex", "age": 65}

// Rex additions
undefined                  // absence of value
0xFF   0b1010              // hex and binary numbers
{name: "Rex", age: 65}    // bare identifier keys
[1 2 3]  {a: 1 b: 2}      // commas are optional
{(field): "value"}         // computed key expressions
```

### Navigation

Dots navigate into nested structures. Parenthesized expressions handle dynamic keys:

```rex
user.name                  // static key
user.address.street        // nested
map.(key)                  // dynamic key (variable)
config.(headers.x-action)  // dynamic key (expression)
self                       // implicit value in current scope
self@2                     // parent scope's self
```

### Assignment

```rex
x = 42
obj.key = "value"
x += 1    x -= 5    x *= 2
```

### Operators

```rex
// Arithmetic
x + y    x - y    x * y    x / y    x % y    -x

// Comparison (returns value on success, undefined on failure)
age > 18    age <= 65    x == y    x != y

// Existence (short-circuit on defined vs undefined)
a and b       // b if both defined
a or b        // first defined value
a nor b       // b if a is undefined

// Value operators (boolean algebra / bitwise)
a & b    a | b    a ^ b    ~a
```

### Control Flow

```rex
when age > 18 do
  allow(self)
end

unless authorized do
  deny()
end

when string(value) do
  handle-string(self)
else when number(value) do
  handle-number(self)
else
  handle-other()
end
```

### Iteration

```rex
// For loops
for v in [1, 2, 3] do process(v) end
for k, v in obj do log(k, v) end

// Ranges
1..10              // [1, 2, 3, ..., 10]

// Array comprehension
[v * 2 for v in [1, 2, 3]]           // [2, 4, 6]

// Object comprehension
{(k): v * 10 for k, v in {a: 1, b: 2}}  // {a: 10, b: 20}

// Filtering (undefined values are excluded)
[v % 2 == 0 and v for v in 1..10]    // [2, 4, 6, 8, 10]
```

### Type Predicates

Return the value if it matches the type, `undefined` otherwise:

```rex
when n = number(input) do
  total += n
else when s = string(input) do
  log("got string: " + s)
end
```

For the complete syntax and semantics, see the [Language Reference](language.md).

## Example Programs

### Fibonacci

```rex
max = max or 100

fibs = []
i = 0
a = 1
b = 1
while a <= max do
  fibs.(i) = a
  i += 1
  c = a + b
  a = b
  b = c
end

fibs
```

### Sieve of Eratosthenes

```rex
max = max or 100

composites = {}
n = 2
while n * n <= max do
  unless composites.(n) do
    m = n * n
    while m <= max do
      composites.(m) = true
      m += n
    end
  end
  n += 1
end

[composites.(self) nor self in 2..max]
```

## Compilation

Rex compiles to `rexc` — a compact bytecode that serializes as a UTF-8 string. You can store it in JSON, diff it, and transmit it like any other string data. Interpreters execute `rexc` directly.

For the full bytecode specification, see the [Bytecode Format](rexc-bytecode.md).

## Getting Started

Install the CLI:

```sh
bun add -g @creationix/rex
```

Use it:

```sh
rex fibonacci.rex                    # evaluate and output JSON result
rex -e 'max = 200' fibonacci.rex     # set a variable before running
rex -c --expr "when x do y end"      # compile to rexc bytecode
rex --expr "a and b" --ir            # show lowered IR
```

Zero-install alternatives:

```sh
bunx @creationix/rex --expr "when x do y end"
npx -y @creationix/rex -- --expr "when x do y end"
```

## Programmatic API

```ts
import { compile, parseToIR, optimizeIR, encodeIR } from "@creationix/rex";

const source = "when x do y else z end";

const encoded = compile(source);
const optimized = compile(source, { optimize: true });

const ir = parseToIR(source);
const optimizedIR = optimizeIR(ir);
const reEncoded = encodeIR(optimizedIR);
```

## Tooling

### VS Code Extension

The [Rex for VS Code](packages/vscode-rex) extension provides:

- Syntax highlighting for `.rex` and `.rexc` files
- Parser-backed diagnostics
- Outline, Go to Definition, and Find References
- Domain-aware completion and hover via `.config.rex`

## Documentation

- [Language Reference](language.md) — complete syntax and semantics
- [Bytecode Format](rexc-bytecode.md) — `rexc` encoding specification
- [Contributing](CONTRIBUTING.md) — repo layout, development workflow, architecture
