# 🦖 Rex

<picture align="right">
  <source media="(prefers-color-scheme: dark)" srcset="img/rex-mascot-dark.png">
  <source media="(prefers-color-scheme: light)" srcset="img/rex-mascot-light.png">
  <img alt="Rex mascot" src="img/rex-mascot-light.png" align="right" width="200">
</picture>

Programmable JSON. Small arms, big bite.

Rex is a compact expression language for configuration and data-driven logic. It is a superset of JSON with high-level syntax (`when`, `unless`, `and`, `or`, assignment, loops, comprehensions).

Rex covers two common use-case styles:

- **Templated data:** generate structured values from JSON-like templates with lightweight logic.
- **General-purpose decision logic:** write compact policy/router/transform rules as little snippets of logic.

Use Rex when JSON alone is too static, but embedding a full scripting runtime is too heavy.

## What Rex Is

In practice, Rex works like this:

- Start with normal JSON-shaped data.
- Add template-style dynamics while keeping a structured-data result.
- Add just enough logic for real configs (`when`, `unless`, `and`, `or`, loops, comprehensions).
- Compile once to compact `rexc` bytecode for storage, transport, and fast evaluation.

## Where Rex Fits

Rex is a strong fit for:

- HTTP edge routing and middleware policy
- Request/response shaping and header logic
- Feature flags and rollout rules
- Validation and normalization pipelines
- Data-driven rules where full scripting is too much

## Core Mental Model: Existence

Rex uses **existence**, not truthiness. Only `undefined` means “absent.”

All JSON values (including 0, false, and null) are existing values.  Only `undefined` does not exist.

```rex
0 or "fallback"         // => 0
false or "fallback"     // => false
null or "fallback"      // => null
undefined or "fallback" // => "fallback"
```

This drives the language:

- Comparisons return value-or-`undefined`
- `when` / `unless` branch on defined-vs-`undefined`
- `and` / `or` / `nor` short-circuit on existence

## Quick Language Tour

### 1) Read and write data

```rex
user.name
config.(headers.x-action)

status = 200
headers.content-type = "application/json"
old = count := count + 1
```

### 2) Branch with value-or-absence

```rex
when token and token == config.api-token do
  headers.x-auth = "ok"
else
  status = 401
end
```

### 3) Build collections declaratively

```rex
// Array comprehension with filtering
[v % 2 == 0 and v for v in 1..10]

// Object comprehension
{(k): v * 10 for k, v in scores}
```

### 4) Type-check inline

```rex
when n = number(input) do
  total += n
else when s = string(input) do
  log("got string: " + s)
end
```

## Runtime Model

Rex runtimes are gas-bounded: evaluation ends with either a value or a gas-limit failure.

The embedding domain decides how to use Rex (final value, side effects, or both).

For precise semantics and edge-case behavior, see the [Language Reference](language.md).

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

## Example Programs

### Fibonacci

```rex
// Allow host or CLI to override max, but default to 100
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
