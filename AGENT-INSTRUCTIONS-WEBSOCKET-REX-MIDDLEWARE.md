# Instructions: Rex-Powered WebSocket Middleware

## Goal

Make Rex scripts the middleware layer for WebSocket connections in rex-serve. Rex programs mediate between system events (file changes, database writes) and WebSocket subscribers — handling auth, filtering, transformation, and routing.

## Architecture Overview

```
System Events                Rex Middleware              WebSocket Clients
─────────────                ──────────────              ─────────────────
file change ──┐
db.set ───────┼──▶  _ws.rex (per-event)  ──▶  broadcast to subscribers
db.delete ────┘     filters, transforms,
                    routes events

ws connect ──────▶  _ws.rex (on-connect)  ──▶  accept/reject + init state
ws message ──────▶  _ws.rex (on-message)  ──▶  response + side effects
ws close ────────▶  _ws.rex (on-close)    ──▶  cleanup
```

## Script Lifecycle

A WebSocket middleware file (`_ws.rex` or `routes/_ws.rex`) handles multiple event types in a single script. The `event.type` variable distinguishes them:

```rex
/* routes/_ws.rex — WebSocket middleware */

/* Authentication on connect */
when event.type == "connect" do
  unless headers.authorization do
    return {reject: true, reason: "unauthorized"}
  end
  /* Initialize per-connection state */
  return {
    accept: true
    state: {user: headers.authorization, subscriptions: ["articles"]}
  }
end

/* Incoming message from client */
when event.type == "message" do
  msg = json.parse(event.data)
  when msg.action == "subscribe" do
    state.subscriptions = state.subscriptions or []
    /* TODO: how to append to array? */
    return {state: state}
  end
end

/* System event (file change, db write) */
when event.type == "file-change" do
  return {broadcast: true, data: {type: "reload", path: event.path}}
end

when event.type == "db-set" do
  /* Only broadcast article changes to subscribers */
  when event.key and string(event.key) do
    /* Filter: check if connection is subscribed to this topic */
    return {broadcast: true, data: {type: "update", key: event.key, value: event.value}}
  end
end

/* Cleanup on disconnect */
when event.type == "close" do
  log.info("client disconnected: " + state.user)
end
```

## State Management

This is the central design question. Rex programs are stateless — each invocation gets a fresh variable scope. But WebSocket connections are inherently stateful (session data, subscriptions, cursor positions). Three levels of state are needed:

### 1. Per-connection state (session)

State that persists across messages on the same WebSocket connection. This is the equivalent of session data.

**Proposed mechanism**: The `state` variable. The middleware receives `state` as a pre-populated var on each invocation. The returned object's `state` field becomes the new state for the next invocation on this connection.

```rex
/* On connect: initialize state */
when event.type == "connect" do
  return {accept: true, state: {user: principal, count: 0}}
end

/* On message: read and update state */
when event.type == "message" do
  state.count = state.count + 1
  return {state: state, reply: {echo: event.data, count: state.count}}
end
```

The server stores the state object (a `RexValue`) per connection. On each event, it injects `state` as a var, runs the script, and captures the returned `state` field as the new state. This is pure — no mutation across invocations, just functional state threading.

**Implementation**: In the WebSocket handler, maintain a `HashMap<ConnectionId, RexValue>` for per-connection state. Before running the Rex script, inject `state` into `Context.vars`. After running, extract the `state` field from the return value and store it.

### 2. Shared state (global across connections)

State shared between all connections — subscriber counts, rate limiting, feature flags. This is harder because Rex programs can't share mutable state.

**Proposed mechanism**: A new `kv` namespace — an in-memory key-value store with pub/sub, designed for ephemeral shared state. This is distinct from `db` (SQLite, persistent, disk-backed). Think of it as an embedded Redis.

```rex
/* Rate limiting */
rate-key = `rate:${state.user}`
count = kv.get(rate-key) or 0
when count > 100 do
  return {reject: true, reason: "rate_limited"}
end
kv.set(rate-key, count + 1, 60)  /* TTL: 60 seconds */

/* Pub/sub: publish an event to a channel */
kv.publish("articles", {action: "created", slug: input.slug})

/* Track online users */
kv.set(`online:${state.user}`, true, 300)  /* 5-min heartbeat TTL */
online = kv.keys("online:*")
```

### `kv` vs `db` — two tiers of state

| | `kv` (in-memory) | `db` (SQLite) |
|---|---|---|
| **Persistence** | Ephemeral — lost on restart | Persistent — survives restarts |
| **Speed** | Microseconds (HashMap) | Milliseconds (disk I/O) |
| **Use cases** | Sessions, rate limits, pub/sub, online presence, locks | Articles, users, API keys, audit logs |
| **Pub/sub** | Yes — `kv.publish`/`kv.subscribe` | No |
| **TTL** | Yes — auto-expire keys | No |
| **Capacity** | Bounded by memory | Bounded by disk |

### `kv` opcodes

```
kv.get(key)                     → value or none
kv.set(key, value)              → true
kv.set(key, value, ttl)         → true          (TTL in seconds)
kv.delete(key)                  → true
kv.keys(prefix)                 → [string]      (list keys matching prefix)
kv.incr(key)                    → integer       (atomic increment, returns new value)
kv.publish(channel, data)       → integer       (number of subscribers notified)
```

### `kv.subscribe` and WebSocket integration

The `kv.subscribe` opcode is unique — it's used in the WebSocket `connect` handler to register interest in channels. When a message is published to a subscribed channel, the WebSocket middleware receives it as an event:

```rex
/* _ws/connect.rex */
kv.subscribe("articles")
kv.subscribe(`user:${state.user}`)
return {accept: true, state: {user: principal}}

/* _ws/publish.rex — triggered when a subscribed channel receives data */
/* event.channel = "articles", event.data = {action: "created", ...} */
return {reply: json.stringify(event.data)}
```

The publish side happens from any Rex program — an HTTP handler, middleware, or another WebSocket handler:

```rex
/* routes/api/articles.rex — POST handler */
db.set("article:" + input.slug, json.stringify(record))
kv.publish("articles", {action: "created", slug: input.slug, title: input.title})
return {ok: true, slug: input.slug}
```

This naturally connects HTTP writes to WebSocket subscribers without shared mutable state — `kv.publish` is a fire-and-forget event, and `kv.subscribe` registers the connection for future events.

### Implementation

The `kv` store is a single `Arc<Mutex<KvStore>>` in `AppState`:

```rust
struct KvStore {
    data: HashMap<String, KvEntry>,
    channels: HashMap<String, Vec<broadcast::Sender<String>>>,
}

struct KvEntry {
    value: String,  // JSON-serialized
    expires_at: Option<Instant>,
}
```

A background task runs every ~1 second to evict expired keys. The `publish` opcode sends on all matching channel senders. The WebSocket handler holds receivers for subscribed channels.

### 3. The case for user-defined functions

The current Rex language has no user-defined functions. For HTTP handlers, this is fine — each handler is a short independent script. For WebSocket middleware, the lack of functions creates repetition:

```rex
/* Without functions: repeated auth check logic */
when event.type == "message" do
  /* Must inline the auth check — can't call a shared function */
  unless state.user do
    return {close: true, reason: "not authenticated"}
  end
  /* ... handle message ... */
end

when event.type == "db-set" do
  /* Same auth check repeated */
  unless state.user do
    return {close: true, reason: "not authenticated"}
  end
  /* ... handle event ... */
end
```

**With functions, this becomes:**

```rex
check-auth = fn do
  unless state.user do
    return {close: true, reason: "not authenticated"}
  end
end

when event.type == "message" do
  check-auth()
  /* ... handle message ... */
end

when event.type == "db-set" do
  check-auth()
  /* ... handle event ... */
end
```

**Assessment**: Functions are not strictly *needed* — the repetition is manageable for typical WebSocket middleware (5-20 lines per event type). The `when/return` guard pattern already provides clean early exits. Functions would help with:
- Shared validation logic across event types
- Complex data transformations reused in multiple places
- Recursive algorithms (tree traversal, etc.)

However, functions introduce significant complexity: closures, scope capture, recursion, stack depth. For the WebSocket use case, a simpler alternative might work better:

**Alternative: event-specific scripts.** Instead of one `_ws.rex` handling all events, use separate files:
- `_ws/connect.rex` — connection handler
- `_ws/message.rex` — incoming message handler  
- `_ws/close.rex` — disconnection handler
- `_ws/file-change.rex` — file change events
- `_ws/db-set.rex` — database write events

Each script is small and focused. Shared logic between them can go through middleware vars (the same mechanism as HTTP middleware). This keeps Rex function-free while avoiding repetition.

## Event Types and Variables

### Connection events

| Event type | Direction | Variables |
|---|---|---|
| `connect` | client → server | `headers`, `query`, `path`, `ip` (same as HTTP) |
| `message` | client → server | `event.data` (string), `state` |
| `close` | client → server | `event.code`, `event.reason`, `state` |

### System events

| Event type | Direction | Variables |
|---|---|---|
| `file-change` | system → script | `event.path`, `event.kind` ("create"/"modify"/"remove"), `state` |
| `db-set` | system → script | `event.table`, `event.key`, `event.value`, `state` |
| `db-delete` | system → script | `event.table`, `event.key`, `state` |

### Return value convention

The script's return value controls what happens:

| Return field | Meaning |
|---|---|
| `accept: true` | Accept WebSocket connection (connect only) |
| `reject: true` | Reject connection with `reason` |
| `reply: value` | Send a message back to the client |
| `broadcast: true` | Send `data` to all connected clients |
| `state: value` | Update per-connection state |
| `close: true` | Close the connection with optional `reason` |
| `none` / no return | No action (suppress event) |

## Implementation Plan

### Phase 1: System events on existing WebSocket

Extend the current `/__reload` WebSocket to run Rex middleware on file changes:

1. Look for `routes/_ws.rex` on startup
2. On file change, run the middleware with `event.type = "file-change"` and `event.path`
3. If the script returns `{broadcast: true, data: ...}`, send `data` to all clients
4. If it returns `none`, suppress the event (current behavior: always broadcast)

### Phase 2: In-memory KV store with pub/sub

Add the `kv` opcode namespace:

1. Create `KvStore` struct with `HashMap<String, KvEntry>` + `HashMap<String, Vec<Sender>>`
2. Add `kv.*` opcodes: get, set (with optional TTL), delete, keys, incr, publish
3. Register in `AppState` as `Arc<Mutex<KvStore>>`
4. Background tokio task for TTL eviction (every 1s, sweep expired keys)
5. Wire `kv.publish` to broadcast channels

### Phase 3: Database event emission + KV pub/sub integration

Connect database writes to the pub/sub system:

1. After each `db.set`/`db.delete`, optionally `kv.publish("db", {key, value})`
2. Or let Rex programs explicitly publish — more flexible, less magic
3. WebSocket handler subscribes to channels via `kv.subscribe` in the connect script

### Phase 4: Client messages and per-connection state

1. Upgrade the WebSocket handler to receive client messages
2. Maintain per-connection state in a `HashMap<ConnectionId, RexValue>`
3. On client message, run Rex middleware with `event.type = "message"` and inject `state`
4. Handle `reply`, `broadcast`, `close`, and `state` fields in the return value

### Phase 4: Event-specific scripts (optional)

If the single-file approach gets unwieldy:
1. Look for `routes/_ws/connect.rex`, `routes/_ws/message.rex`, etc.
2. Run the appropriate script for each event type
3. Share state through the same `state` var mechanism

## Files to Change

| File | Change |
|---|---|
| `crates/rex-serve/src/server.rs` | Extend WebSocket handler with Rex middleware execution |
| `crates/rex-serve/src/kv.rs` | New: in-memory KV store with TTL + pub/sub channels |
| `crates/rex-serve/src/opcodes.rs` | Add `kv.*` opcodes, optionally emit db events |
| `crates/rex-serve/src/handler.rs` | Extract `run_rex_program` for reuse by WebSocket handler |
| `crates/rex-serve/src/config.rs` | Optional: `[websocket]` config section |
| `examples/knowledge-base/routes/_ws.rex` | Example WebSocket middleware |

## Open Questions

1. **Should `state` be a HostObject or a plain RexValue?** A HostObject would allow `state.x = y` mutations that persist. A plain Object would require returning the full state on each event. HostObject is more natural but adds complexity.

2. **How to handle backpressure?** If Rex middleware is slow, events queue up. Should there be a per-connection event buffer limit?

3. **Should the HTTP middleware chain also run for WebSocket events?** The `connect` event goes through HTTP middleware (for auth). But subsequent events don't have HTTP headers. Should per-connection state carry the auth result from the middleware chain?

4. **Do we need pub/sub topics?** The current model broadcasts to all clients. A topic/channel model (`subscribe("articles")`, `subscribe("users")`) would be more efficient. This could be a convention on `state.subscriptions` without new language features.

5. **User-defined functions**: Are they needed for WebSocket middleware, or can the event-specific script pattern + middleware vars cover the use cases? Functions would be a significant language addition — closures, scope capture, recursion. The simpler alternative (separate files per event type) may be sufficient.
