# 🦖 Rex

<picture align="right">
  <source media="(prefers-color-scheme: dark)" srcset="img/rex-mascot-dark.png">
  <source media="(prefers-color-scheme: light)" srcset="img/rex-mascot-light.png">
  <img alt="Rex mascot" src="img/rex-mascot-light.png" align="right" width="200">
</picture>

Programmable JSON. Small arms, big bite.

Rex is a compact expression language for configuration and data-driven logic. It is a superset of JSON with `when`, `unless`, `and`, `or`, `return`, assignment, loops, comprehensions, and template literals.

Use Rex when JSON alone is too static, but embedding a full scripting runtime is too heavy.

## At a Glance

The same HTTP routing logic — in JavaScript, in Rex, and as compiled bytecode:

**JavaScript** — a full runtime, cold-start overhead, a separate deploy artifact

```js
export function handle({ headers, method, db, json }) {
  if (!headers.authorization) {
    return { status: 401, body: { ok: false, error: "unauthorized" } }
  }
  if (method === "GET") {
    const articles = db.list("article:").map(e => json.parse(e.value))
    return { status: 200, body: { ok: true, articles } }
  }
  return { status: 405, body: { ok: false, error: "method_not_allowed" } }
}
```

**Domain file** (`articles.rexd`) — declares host bindings; enables shortcode rewriting at compile time

```rex
extern method = string
extern mut status = integer
extern headers = {*: string}

extern db.list(prefix: string) -> [{key: string, value: string}]
extern json.parse(text: string) -> some
```

**Rex** (`articles.rex`) — readable, storable, evaluates against your domain's bindings

```rex
unless headers.authorization do
  status = 401
  return {ok: false, error: "unauthorized"}
end

when method == "GET" do
  return {ok: true, articles: [json.parse(e.value) for e in db.list("article:")]}
end

status = 405
{ok: false, error: "method_not_allowed"}
```

**Compiled REXC bytecode** — a single UTF-8 string (~190 bytes); `db.list` → `dl`, `json.parse` → `jp`

```sh
rex compile --domain articles.rexd articles.rex
```

```
{?((headers$d,authorization)no'x{=status$cy+;{1E^f'c,unauthorized}})?((eq%method$3,GET);O{-^t'8,articles>[(dl%8,article:)e$(jp%(e$5,value))]})=status$cG+{2,okf'5,errori,method_not_allowed}}
```

Store this string in a database column, embed it in a JSON config field, diff it in git, and evaluate it anywhere Rex runs — no AST, no bytecode files, no separate runtime.

> **Side-by-side in GitHub Markdown:** GitHub doesn't support multi-column layouts with syntax highlighting. The standard approach (used above) is clearly-labelled sequential blocks. For plain-text comparison without highlighting, an HTML `<table>` with `<pre>` cells works.

## Core Mental Model: Existence

Rex uses **existence**, not truthiness. Only `none` means "absent." All JSON values — including `0`, `false`, `null`, and `""` — are existing values.

```rex
0 or "fallback"      // => 0         (0 is a value)
false or "fallback"  // => false     (false is a value)
none or "fallback"   // => "fallback" (none is absent)
```

This drives everything: comparisons return value-or-`none`, `when`/`unless` branch on existence, `and`/`or` short-circuit on existence.

## Quick Tour

```rex
/* Guard-style HTTP handler */
unless headers.authorization do
  res.status = 401
  return {ok: false, error: "unauthorized"}
end

when method == "GET" do
  items = [json.parse(a.value) for a in db.list("article:")]
  return {ok: true, articles: [{slug: a.slug, title: a.title} for a in items]}
end

when method == "POST" do
  input = json.parse(body)
  db.set(`article:${input.slug}`, json.stringify(input))
  res.status = 201
  return {ok: true, slug: input.slug}
end

res.status = 405
{ok: false, error: "method_not_allowed"}
```

## Getting Started

### Rust CLI (recommended)

```sh
# from repo root
cargo run -p rex-cli -- run fibonacci.rex
cargo run -p rex-cli -- compile --expr "when x do y end"
cargo run -p rex-cli -- check routes/ --domain server.rexd
```

### Node/Bun CLI

```sh
bun add -g @creationix/rex
rex fibonacci.rex
rex -c --expr "when x do y end"
```

## File Formats

| Extension | Format | Description |
|---|---|---|
| `.rex` | Rex source | The high-level language — compiled to `.rexc` bytecode |
| `.rexd` | Rex declarations | Domain interface files — `type` and `extern` declarations for tooling |
| `.rexc` | REXC bytecode | Compiled Rex — variables, control flow, opcodes. Superset of RX |
| `.rx` | RX data | Data-only subset of REXC — JSON-compatible values encoded as compact UTF-8 |

RX is to REXC what JSON is to JavaScript: a pure data format that happens to be valid in the larger language. You can store RX in JSON string fields, diff it, and transmit it like any other text.

## Rex in Practice: rex-serve

The [rex-serve](crates/rex-serve) demo embeds Rex as the scripting layer for an HTTP server. Every page is a `.rex` file. Run the self-guided tour:

```sh
# from repo root
cargo run -p rex-serve -- --dir examples/knowledge-base --port 4000
# Open http://localhost:4000
```

Features: filesystem routing, middleware chains, tagged template literals with auto-escaping, domain-aware compilation, in-memory KV store with pub/sub, WebSocket channels with Rex transform scripts, hot reload with type checking, Tokyo Night syntax highlighting, and a live multi-user cursor demo.

For a detailed review of what worked well during development, see [rex-serve/REVIEW.md](crates/rex-serve/REVIEW.md).

## Packages

### Rust (active development)

| Crate | Description |
|---|---|
| [rex-core](crates/rex-core) | Lexer, parser, CST, lowerer, bytecode encoder/decoder, interpreter, type checker |
| [rex-cli](crates/rex-cli) | CLI: `compile`, `run`, `inspect`, `decompile`, `check`, REPL |
| [rex-serve](crates/rex-serve) | HTTP server with filesystem routing, WebSocket pub/sub, KV store ([tour app](examples/knowledge-base)) |
| [rex-node](crates/rex-node) | Node.js native addon via NAPI |
| [rex-luajit](crates/rex-luajit) | LuaJIT FFI bindings |

### TypeScript

| Package | Description |
|---|---|
| [vscode-rex](packages/vscode-rex) | VS Code extension (LSP client, syntax highlighting) |
| [rex-ts](packages/rex-ts) | TypeScript tagged template utilities for Rex |

## Documentation

| Document | Description |
|---|---|
| [Language Reference](docs/language.md) | Complete syntax and semantics |
| [REXC Bytecode](docs/rexc-bytecode.md) | Bytecode format specification |
| [RX Data Format](docs/rx-format.md) | JSON-compatible data encoding |
| [Type System](docs/rex-types.md) | Type inference, `.rexd` declarations, diagnostics |
| [rex-serve Review](crates/rex-serve/REVIEW.md) | Lessons from embedding Rex in a real HTTP server |
| [Contributing](CONTRIBUTING.md) | Repo layout, development workflow |

## Example Programs

### Fibonacci

```rex
max = max or 100
a = 1
b = 1
fibs = []
i = 0
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
[composites.(n) != true and n for n in 2..max]
```
