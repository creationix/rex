# 🦖 Rex

<picture align="right">
  <source media="(prefers-color-scheme: dark)" srcset="img/rex-mascot-dark.png">
  <source media="(prefers-color-scheme: light)" srcset="img/rex-mascot-light.png">
  <img alt="Rex mascot" src="img/rex-mascot-light.png" align="right" width="200">
</picture>

Programmable JSON. Small arms, big bite!

Rex is a data format with three containers: `{}` for objects, `[]` for arrays, `()` for code. Any valid JSON is already valid Rex — you just add parentheses where you need logic. Source compiles to compact JSON bytecode that you can store, serialize, and diff like any other data.

If you like tools that feel inevitable once you see them, you’re in the right place.

## What Problem This Solves

You have structured data. JSON handles it fine. Then you need a little logic — a lookup, a conditional, a fallback — and suddenly you're choosing between:

- Hardcoding every case — rigid, verbose, duplicated across rules.
- Embedding a scripting language — Lua, JS, WASM — a whole new runtime and security surface for a few if statements.
- Writing a custom DSL — fun at first, maintenance forever.

Rex is the fourth option. Your data stays JSON. You add `()` where you need logic. The compiled output is JSON arrays — storable, serializable, diffable.


## Core Language

Rex is designed to be used in various problem domains with a flexible core.

- `when`, `unless`, `do` - conditional control flow
- `alt`, `all` - short-circuit operators
- `set`, `delete` - place operations
- `eq`, `neq`, `gt`, `gte`, `lt`, `lte` - comparison (return value or `undefined`)
- `and`, `or`, `not`, `xor` - boolean / bitwise operators
- `add`, `sub`, `mul`, `div`, `mod`, `neg` - arithmetic
- `string`, `number`, `object`, `array`, `boolean`, `bytes` - type predicates
- `literal` - escape hatch for data that looks like code

## Example

A common pattern: hundreds of named actions, each mapped to a handler. In plain JSON, every action needs its own rule:

```ts
export default [
  { when: [{ condition: "header", key: "x-action", equals: "create-user" }],
    do:   [{ action: "set-header", key: "x-handler", value: "users/create" }] },
  { when: [{ condition: "header", key: "x-action", equals: "delete-user" }],
    do:   [{ action: "set-header", key: "x-handler", value: "users/delete" }] },
  // ... 200+ more rules, each repeating the same structure
]
```

Every entry duplicates the same conditional logic. At 200 actions, that's 200 copy-pasted rules.

In Rex, the data is a lookup table and the logic is written once:

```rex
actions = {
  create-user:       "users/create"
  delete-user:       "users/delete"
  update-profile:    "users/update-profile"
  create-order:      "orders/create"
  process-payment:   "payments/process"
  send-notification: "notifications/send"
  // ... 200+ more entries
}

when handler = actions.(headers.x-action) do
  headers.x-handler = handler
end
```

Adding a new action is one line in the table. The logic doesn't change.

When compiled this uses Rex's compact encoding. In normal debug mode, variable names are preserved (`actions`, `handler`):

```rexc
(%=actions${create-user:c,users/createdelete-user:c,users/deleteupdate-profile:k,users/update-profilecreate-order:d,orders/createprocess-payment:g,payments/processsend-notification:i,notifications/send}?(=handler$(actions$(headers$x-action:))s=(headers$x-handler:)handler$))
```

Optimized mode inlines the lookup table and removes the extra named table assignment, then uses `self` for the matched value in the branch body:

```rexc
?(({create-user:c,users/createdelete-user:c,users/deleteupdate-profile:k,users/update-profilecreate-order:d,orders/createprocess-payment:g,payments/processsend-notification:i,notifications/send}(headers$x-action:))l=(headers$x-handler:)@)
```

## Compiler Helper

Use the built-in CLI helper to compile high-level Rex to compact encoding instead of deriving rexc by hand:

```sh
bun run rex:compile --expr "when x do y end"
bun run rex:compile --file input.rex
cat input.rex | bun run rex:compile
```

Use `--ir` to emit lowered IR JSON.

## IR Optimizer

Rex now includes an IR-to-IR optimizer pass.

### API

```ts
import { parseToIR, optimizeIR, compile } from "./packages/rex-lang/rex.ts";

const ir = parseToIR("1 + 2");
const optimized = optimizeIR(ir);

const encoded = compile("1 + 2", { optimize: true });
```

### Example: constant fold

Input IR:

```json
{ "type": "binary", "op": "add", "left": { "type": "number", "raw": "1", "value": 1 }, "right": { "type": "number", "raw": "2", "value": 2 } }
```

Optimized IR:

```json
{ "type": "number", "raw": "3", "value": 3 }
```

### Example: constant propagation + navigation fold

Source:

```rex
t = {a: 1, b: 2}
t.b
```

Optimized IR (shape):

```json
{
  "type": "program",
  "body": [
    { "type": "assign", "op": "=", "place": { "type": "identifier", "name": "t" }, "value": { "type": "object", "entries": [ { "key": { "type": "key", "name": "a" }, "value": { "type": "number", "raw": "1", "value": 1 } }, { "key": { "type": "key", "name": "b" }, "value": { "type": "number", "raw": "2", "value": 2 } } ] } },
    { "type": "number", "raw": "2", "value": 2 }
  ]
}
```

The optimizer is conservative and only applies simple, safe folds (constants, literal navigation, and statically decidable conditionals).
