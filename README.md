# 🦖 Rex

<picture align="right">
  <source media="(prefers-color-scheme: dark)" srcset="img/rex-mascot-dark.png">
  <source media="(prefers-color-scheme: light)" srcset="img/rex-mascot-light.png">
  <img alt="Rex mascot" src="img/rex-mascot-light.png" align="right" width="200">
</picture>

Programmable JSON. Small arms, big bite.

Rex is a compact expression language for configuration and data-driven logic. It is a superset of JSON with high-level infix syntax (`when`, `unless`, `and`, `or`, assignment, loops, comprehensions) that compiles to a compact bytecode string format called `rexc`.

If you need data-first configs with just enough logic—without embedding a full scripting runtime—Rex is built for that.

## Why Rex

When JSON-only configs hit real-world logic, teams usually end up with one of these:

- Massive duplicated rules
- Embedded JavaScript/Lua/WASM runtimes
- A custom DSL that keeps growing forever

Rex keeps the model data-oriented and explicit:

- JSON-like literals and object/array ergonomics
- Existence-based control flow (`undefined` means absence)
- Compact, serializable `rexc` output
- Domain-agnostic core language

## Quick Start

Install CLI once:

```sh
bun add -g @creationix/rex
```

Compile Rex directly:

```sh
rex --expr "when x do y end"
rex --file input.rex
cat input.rex | rex
rex --expr "a and b" --ir
```

Run without global install:

```sh
bunx @creationix/rex --help
bunx @creationix/rex --expr "when x do y end"

npx -y @creationix/rex -- --help
npx -y @creationix/rex -- --expr "when x do y end"
```

Or, from repo root, use workspace scripts:

```sh
bun run rex:compile --expr "when x do y end"
bun run rex:compile --file input.rex
cat input.rex | bun run rex:compile
bun run rex:compile --expr "a and b" --ir
```

## Language Snapshot

Rex uses **existence semantics** (defined vs `undefined`), not truthiness.

```rex
0 or "fallback"         // => 0
false or "fallback"     // => false
null or "fallback"      // => null
undefined or "fallback" // => "fallback"
```

Core forms:

```rex
// conditionals
when cond do expr end
unless cond do expr else other end

// assignment
x = 42
obj.key += 1

// existence operators
a and b
a or b

// value operators (boolean/bitwise depending on type)
a & b
~a

// depth-aware self
self
self@2

// loops and comprehensions
for v in items do v end
[v in items ; v * 2]
{k, v in obj ; (k): v}
```

## Example: table-driven routing

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

Normal compile output (names preserved):

```rexc
(%=actions$1p{create-user:c,users/createdelete-user:c,users/deleteupdate-profile:k,users/update-profile}?(=handler$r(actions$(headers$x-action:))s=(headers$x-handler:)handler$))
```

Optimized compile output:

```rexc
?(({create-user:c,users/createdelete-user:c,users/deleteupdate-profile:k,users/update-profile}(headers$x-action:))l=(headers$x-handler:)@)
```

## Programmatic API

```ts
import { compile, parseToIR, optimizeIR, encodeIR } from "./packages/rex-lang/rex.ts";

const source = "when x do y else z end";

const encoded = compile(source);
const optimizedEncoded = compile(source, { optimize: true });

const ir = parseToIR(source);
const optimizedIR = optimizeIR(ir);
const reEncoded = encodeIR(optimizedIR);
```

## Tooling

### Rex compiler package

- Location: `packages/rex-lang`
- Includes grammar, parser/lowering, optimizer, and encoder

Useful commands:

```sh
cd packages/rex-lang
bun test
bun run build:grammar
```

### VS Code extension

- Location: `packages/vscode-rex`
- Adds syntax highlighting for `.rex` and `.rexc`

Useful commands:

```sh
cd packages/vscode-rex
bun test
bun run build
bun run reinstall
```

## Repo Layout

- `packages/rex-lang` — language/compiler implementation
- `packages/vscode-rex` — VS Code grammar + tokenizer + extension
- `high-level.md` — complete high-level language reference
- `encoding.md` — `rexc` encoding format reference

## Development Workflow

From repo root:

```sh
bun run rex:compile --expr "when x do y end"
bun run rex:verify-docs
```

Installable CLI (`rex`):

```sh
bun add -g @creationix/rex
rex --help
rex --expr "when x do y end"
rex --expr "a and b" --ir
```

Zero-install CLI:

```sh
bunx @creationix/rex --expr "when x do y end"
npx -y @creationix/rex -- --expr "when x do y end"
```

When editing grammar (`packages/rex-lang/rex.ohm`):

```sh
cd packages/rex-lang
bun run build:grammar
bun test
```

When editing VS Code grammar/tokenizer:

```sh
cd packages/vscode-rex
bun test
bun run build
```

## Status

Rex now uses high-level infix syntax as the primary source form. The old s-expression-style representation is no longer the user-facing language in this repo.
