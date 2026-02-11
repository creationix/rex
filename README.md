# 🦖 Rex

<picture align="right">
  <source media="(prefers-color-scheme: dark)" srcset="img/rex-mascot-dark.png">
  <source media="(prefers-color-scheme: light)" srcset="img/rex-mascot-light.png">
  <img alt="Rex mascot" src="img/rex-mascot-light.png" align="right" width="200">
</picture>

Programmable JSON. Small arms, big bite!

Rex is a small utility for adding *just enough* flexibility to structured configuration.

Three containers: `{}` for objects, `[]` for arrays, `()` for code.

Any valid JSON is already valid Rex — you just add logic where you need it.

If you like tools that feel inevitable once you see them, you’re in the right place.

## What Problem This Solves

Consider an application with hundreds of actions, each identified by a name and mapped to a handler. In the JSON system, we had tried to be flexible by having rather generic rules that could match on arrays of conditions that map to arrays of actions with different actions and parameters.  But it is still very rigid and verbose. Each new action requires a new rule, and the logic is duplicated across all rules.

```ts
export default [
  { when: [ { condition: "header", key: "x-action", equals: "create-user" } ],
    do: [ { action: "set-header", key: "x-handler", value: "users/create" } ] },
  { when: [ { condition: "header", key: "x-action", equals: "delete-user" } ],
    do: [ { action: "set-header", key: "x-handler", value: "users/delete" } ] },
  { when: [ { condition: "header", key: "x-action", equals: "update-profile" } ],
    do: [ { action: "set-header", key: "x-handler", value: "users/update-profile" } ] },
  { when: [ { condition: "header", key: "x-action", equals: "create-order" } ],
    do: [ { action: "set-header", key: "x-handler", value: "orders/create" } ] },
  { when: [ { condition: "header", key: "x-action", equals: "process-payment" } ],
    do: [ { action: "set-header", key: "x-handler", value: "payments/process" } ] },
  { when: [ { condition: "header", key: "x-action", equals: "send-notification" } ],
    do: [ { action: "set-header", key: "x-handler", value: "notifications/send"  } ] }
  // ... 200+ more actions
]
```

Six entries, six rules — each repeating the same conditional structure. At 200 actions, that's 400+ lines of rules (2,800+ strings, 1000+ objects).

With Rex, the *data* is a table and the logic is one rule:

```ts
// We can use normal TS/JS syntax to define or generate the data.
const actions = {
  'create-user':       'users/create',
  'delete-user':       'users/delete',
  'update-profile':    'users/update-profile',
  'create-order':      'orders/create',
  'process-payment':   'payments/process',
  'send-notification': 'notifications/send'
  // ... 200+ more actions
}
export default [ { code:
  rex`
    // Get handler based on x-action request header and actions lookup table.
    (when (read ${actions} (read req.headers 'x-action'))
      // If there was a matching handler, set it in the x-handler header.
      (write req.headers 'x-handler' self))
  `
} ]
```

At 200 actions, the data is 200 lines and the logic is still two. Adding a new action is one line — no new rules, no new conditions.

When serialized as JSON, this is 14 strings for the code and 400 strings for the data.
When serialized as random-access strings, this is 1 string for the entire data and code combined.

The `rex` template tag can return JSON or random-access string.  For clarity, this is what the JSON output would look like this.  Notice that the large JSON data block is inserted inline without any escaping.  Rex is JSON native.

```json
[ { "code":
  ["$when", ["$read", {
    "create-user": "users/create",
    "delete-user": "users/delete",
    "update-profile": "users/update-profile",
    "create-order": "orders/create",
    "process-payment": "payments/process",
    "send-notification": "notifications/send"
    // ... 200+ more actions
  }, ["$read", ["$headers"], "x-action"]]],
  ["$write", ["$headers"], "x-handler", ["$self"] ]
} ]
```

## Tooling

The included vscode extension provides syntax highlighting for rex.  It supports rex inside markdown code blocks with `rex` language tag, and also rex inside template literals tagged with `rex`.  It even supports res inside typescript inside of markdown code blocks.

![highlighted example](img/highlight-example.png)
