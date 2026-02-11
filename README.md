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
  'create-user':       'users/create'
  'delete-user':       'users/delete'
  'update-profile':    'users/update-profile'
  'create-order':      'orders/create'
  'process-payment':   'payments/process'
  'send-notification': 'notifications/send'
  // ... 200+ more entries
}

// Read the action from the header, lookup the handler in the table,
(when handler=(actions (headers 'x-action'))
  // If a handler was found, write it back to the headers.
  (write headers 'x-handler' handler))
```

Adding a new action is one line in the table. The logic doesn't change.

This compiles to JSON bytecode. Notice the data object embeds directly — no escaping, no wrapping. Rex is JSON-native:

```json
["$when", ["$read", {
  "create-user": "users/create",
  "delete-user": "users/delete",
  "update-profile": "users/update-profile",
  "create-order": "orders/create",
  "process-payment": "payments/process",
  "send-notification": "notifications/send"
}, ["$read", ["$headers"], "x-action"]],
["$write", ["$headers"], "x-handler", ["$self"]]]
```
