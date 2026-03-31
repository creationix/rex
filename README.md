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
- Domain-aware completion and hover via `.rexd`

## Rex in Practice: Lessons from rex-serve

The [rex-serve](packages/rusty-rex/crates/rex-serve) project embeds Rex as the scripting layer for an HTTP server — filesystem-routed `.rex` files as edge functions with middleware, templates, markdown rendering, and a CRUD API. Here's what stood out.

### Three Favorite Features

**1. Existence semantics eliminate an entire class of bugs**

When handling HTTP requests, you constantly deal with optional values — headers that may not exist, query params that may be absent, database lookups that return nothing. In most languages, you need `!= null` or `?? default` checks everywhere and still get bitten by `0` or `""` being falsy. Rex's existence model makes `or` do exactly what you mean:

```rex
/* All of these keep the left value — none of them are "absent" */
api-key = headers.authorization     /* none if missing, string if present */
max = query.limit or 100            /* 0 is a valid limit, won't fall through */
name = user.nickname or user.email  /* "" is a valid nickname */

unless api-key do
  res.status = 401                  /* only fires if truly absent */
end
```

**2. Comprehensions make data transformation effortless**

Mapping, filtering, and reshaping data is the core job of API handlers. Rex comprehensions are more concise than `.map().filter()` chains and read naturally:

```rex
/* Fetch from DB, parse, reshape — one pipeline */
articles = db.list("article:")
items = [json.parse(a.value) for a in articles]
{ok: true, articles: [{slug: a.slug, title: a.title} for a in items]}
```

The `for k, v in obj` variant for objects and `for k of obj` for keys-only means you never need `Object.entries()` or `Object.keys()` — the loop form tells you what you're iterating.

**3. Unified navigation model means one syntax for everything**

Reading a header, accessing a config value, navigating a JSON response, and indexing an array all use the same `.` syntax. Dynamic keys use `.(expr)`. There's no distinction between bracket access and dot access, no special map/dictionary API:

```rex
headers.content-type                /* static key */
config.(env + "-timeout")           /* dynamic key */
users.0.name                        /* array index + property */
routes.(method + " " + path)        /* table lookup */
```

This means Rex programs read like data navigation, which is exactly what HTTP handlers are — navigate the request, transform it, produce a response.

### Pain Points and Planned Fixes

**Lazy maps break when passed across boundaries** (fix: [bytecode-v2 indexed containers](packages/rusty-rex/bytecode-v2.md))

Rex compiles object literals like `{ok: true, slug: input.slug}` as lazy bytecode spans that are only evaluated on access. This is efficient for large data files, but when an object literal is passed as an argument to an opcode (a host-provided function), the opcode receives an opaque `Lazy(span)` it can't read. The same issue appeared in comprehensions: `[{slug: a.slug} for a in items]` produced lazy maps that all resolved `a` to its final loop value instead of each iteration's value. The rex-serve project required adding `force_value()` to the interpreter at opcode boundaries and comprehension boundaries as a workaround.

- [ ] Compiler: emit eager maps when the body references local variables or appears in a call argument position
- [ ] Interpreter: always force values at comprehension boundaries
- [ ] Bytecode v2: explicit indexed vs non-indexed containers make eager/lazy an encoding choice rather than an interpreter guess

**No early return means sequential blocks override each other** (fix: [return statement](packages/rusty-rex/bytecode-v2.md#return))

Rex programs are expression sequences where the last expression's value wins. This forces `when/else` chains for HTTP method dispatch instead of the more natural guard-style pattern:

```rex
/* Current: must use when/else chain so only one branch produces the final value */
when method == "GET" do
  {ok: true, data: items}
else when method == "POST" do
  {ok: true, created: id}
else
  res.status = 405
  {ok: false, error: "method_not_allowed"}
end

/* With return: guard-style, top-to-bottom, early exit */
when method == "GET" do
  return {ok: true, data: items}
end
when method == "POST" do
  return {ok: true, created: id}
end
res.status = 405
{ok: false, error: "method_not_allowed"}
```

The `return` keyword is designed in bytecode-v2 as a postfix `value;` — the `;` tag follows the return value and halts execution, propagating through all enclosing blocks, loops, and conditionals. A bare `return` compiles to `no';` (return none). This was the second most impactful missing feature during rex-serve development: every handler needed restructuring into `when/else` chains to avoid the "last expression wins" behavior.

- [ ] Add `return` keyword to grammar, parser, lowering, and interpreter
- [ ] Bytecode: `;` postfix tag (value precedes the tag, varint reserved for future multi-return)

**String interpolation (solved: template literals)**

Previously, building HTML meant escaped-quote string concatenation. Template literals and tagged templates now solve this:

```rex
/* Before: escaped quotes everywhere */
body = body + "<li><a href=\"/articles/" + slug + "\">" + title + "</a></li>"

/* After: template literals */
body = body + `<li><a href="/articles/${slug}">${title}</a></li>`

/* Tagged template: auto-escapes interpolated values (XSS-safe) */
html`<p>${user-input}</p>`
```

Tagged templates compile to calls with separated static parts and interpolated values. Hosts register tag functions as opcodes — rex-serve's `html` tag auto-escapes interpolations. Nesting is supported: use `html` for escaping user data, untagged backticks for composing safe fragments.

- [x] Template literal syntax added to grammar (backtick-delimited, `${expr}` interpolation)
- [x] Lower to string chains for untagged, calls for tagged
- [x] Lexer: backtick token with brace-depth tracking for nested templates
- [x] rex-serve `html` tagged template with auto-escaping

### How the Type System Would Have Helped

The [type system](rex-types.md) and [`.rexd` domain interface files](rex-types.md#domain-interface-files-rexd) would have caught specific bugs encountered during rex-serve development:

**The "last expression wins" bug** — the type checker could warn when multiple `when` blocks at the top level all produce values, since only the last one's result is used. An "unused value" diagnostic would have caught this immediately instead of requiring debugging of empty API responses.

**Wrong argument types to opcodes** — passing a string where an object was expected (e.g., `template.render(layout, title)` instead of `template.render(layout, {title: title})`) would be caught by the function signatures in `rex-serve.rexd`.

**Property access typos** — `req.headrs` (typo) would get "Unknown property 'headrs'. Did you mean 'headers'?" since `req` has known fields in the `.rexd` declaration.

**The `{*: T}` map type producing `T | none` on lookup** is the right design. Every `headers.x-something` lookup should force a `when` check before use — the type system validates what Rex's existence semantics already encourage.

## Documentation

- [Language Reference](language.md) — complete syntax and semantics
- [Bytecode Format](rexc-bytecode.md) — `rexc` encoding specification
- [Contributing](CONTRIBUTING.md) — repo layout, development workflow, architecture
