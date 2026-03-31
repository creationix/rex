# 🦖 Rex

<picture align="right">
  <source media="(prefers-color-scheme: dark)" srcset="img/rex-mascot-dark.png">
  <source media="(prefers-color-scheme: light)" srcset="img/rex-mascot-light.png">
  <img alt="Rex mascot" src="img/rex-mascot-light.png" align="right" width="200">
</picture>

Programmable JSON. Small arms, big bite.

Rex is a compact expression language for configuration and data-driven logic. It is a superset of JSON with `when`, `unless`, `and`, `or`, `return`, assignment, loops, comprehensions, and template literals.

Use Rex when JSON alone is too static, but embedding a full scripting runtime is too heavy.

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
cd packages/rusty-rex
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

## Rex in Practice: rex-serve

The [rex-serve](packages/rusty-rex/crates/rex-serve) demo embeds Rex as the scripting layer for an HTTP server. Every page is a `.rex` file. Run the self-guided tour:

```sh
cd packages/rusty-rex
cargo run -p rex-serve -- --dir examples/knowledge-base --port 4000
# Open http://localhost:4000
```

Features: filesystem routing, middleware chains, tagged template literals with auto-escaping, domain-aware compilation, in-memory KV store with pub/sub, WebSocket channels with Rex transform scripts, hot reload with type checking, Tokyo Night syntax highlighting, and a live multi-user cursor demo.

For a detailed review of what worked well during development, see [rex-serve/REVIEW.md](packages/rusty-rex/crates/rex-serve/REVIEW.md).

## Packages

### Rust (active development)

| Crate | Description |
|---|---|
| [rex-core](packages/rusty-rex/crates/rex-core) | Lexer, parser, CST, lowerer, bytecode encoder/decoder, interpreter, type checker |
| [rex-cli](packages/rusty-rex/crates/rex-cli) | CLI: `compile`, `run`, `inspect`, `decompile`, `check`, REPL |
| [rex-serve](packages/rusty-rex/crates/rex-serve) | HTTP server with filesystem routing, WebSocket pub/sub, KV store ([tour app](packages/rusty-rex/examples/knowledge-base)) |
| [rex-node](packages/rusty-rex/crates/rex-node) | Node.js native addon via NAPI |
| [rex-luajit](packages/rusty-rex/crates/rex-luajit) | LuaJIT FFI bindings |

### TypeScript (legacy — predates the Rust rewrite)

> **Note:** These packages are from the original TypeScript implementation. The Rust crates above are the active, canonical implementation. The TS packages may not support all current language features.

| Package | Description |
|---|---|
| [rex-lang](packages/rex-lang) | Original TS compiler (Ohm grammar, parser, encoder) |
| [vscode-rex](packages/vscode-rex) | VS Code extension (syntax highlighting, diagnostics) |
| [rex-ts](packages/rex-ts) | TypeScript API bindings |

## Documentation

| Document | Description |
|---|---|
| [Language Reference](language.md) | Complete syntax and semantics |
| [REXC Bytecode](rexc-bytecode.md) | Bytecode format specification |
| [RX Data Format](rx-format.md) | JSON-compatible data encoding |
| [Type System](rex-types.md) | Type inference, `.rexd` declarations, diagnostics |
| [rex-serve Review](packages/rusty-rex/crates/rex-serve/REVIEW.md) | Lessons from embedding Rex in a real HTTP server |
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
