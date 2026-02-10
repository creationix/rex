# 🦖 Rex

Programmable JSON. Small arms, big bite!

Rex is a small utility for adding *just enough* flexibility to structured configuration.

It is designed for cases where static JSON is too rigid, but embedding a general-purpose scripting language (Lua, JavaScript, etc.) would be unnecessary overhead.

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

With Rex, the data is a table and the logic is one rule:

```ts
export default [
  rex`
    actions = {
      'create-user':       'users/create'
      'delete-user':       'users/delete'
      'update-profile':    'users/update-profile'
      'create-order':      'orders/create'
      'process-payment':   'payments/process'
      'send-notification': 'notifications/send'
      // ... 200+ more actions
    }

    // Get handler based on x-action request header and actions lookup table.
    (when handler=(actions (headers 'x-action'))
      // If there was a matching handler, set it in the x-handler header.
      (set headers 'x-handler' handler))
  `
]
```

At 200 actions, the data is 200 lines and the logic is still two. Adding a new action is one line — no new rules, no new conditions.

When serialized as JSON, this is 14 strings for the code and 400 strings for the data.
When serialized as random-access strings, this is 1 string for the entire data and code combined.
