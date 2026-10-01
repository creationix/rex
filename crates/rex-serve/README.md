# rex-serve

An HTTP server that uses [Rex](https://github.com/creationix/rex) scripts as edge functions. Filesystem-routed `.rex` files handle requests with middleware, templates, markdown rendering, and a built-in KV store.

## Quick Start

```sh
cargo run -p rex-serve -- --dir examples/knowledge-base --port 4000
```

Open http://localhost:4000 for a self-guided tour of every feature.

## How It Works

Create `.rex` files in a `routes/` directory. Each file maps to a URL:

```
routes/
  index.rex              → GET /
  health.rex             → GET /health
  api/
    articles.rex         → * /api/articles
    articles/[slug].rex  → * /api/articles/:slug
  style.css              → GET /style.css (static)
```

### Handlers

A handler is a Rex program. It receives request data as variables and returns a response:

```rex
when method == "GET" do
  return {ok: true, data: db.list("items:")}
end
when method == "POST" do
  input = json.parse(body)
  db.set(`item:${input.id}`, json.stringify(input))
  res.status = 201
  return {ok: true}
end
res.status = 405
{ok: false, error: "method_not_allowed"}
```

- Object/array return values are auto-serialized as JSON
- String return values are sent as-is (set `res.headers.content-type` for HTML)
- `none` return produces an empty body

### Dynamic Routes

Files named `[param].rex` capture path segments:

```rex
/* routes/users/[id].rex */
user = db.get(`user:${params.id}`)
when user do return json.parse(user) end
res.status = 404
{error: "not found"}
```

### Middleware

Files named `_middleware.rex` run before every handler in their directory and all subdirectories. They execute root-first:

```rex
/* routes/api/_middleware.rex */
unless headers.authorization do
  res.status = 401
  return {error: "unauthorized"}
end
log.info(`authenticated: ${headers.authorization}`)
```

Variables set by middleware persist into downstream handlers.

### Static Files

Non-`.rex` files are served directly with auto-detected content types. Files/directories starting with `_` are private — never served, but readable by handlers via `fs.read()`.

```
routes/
  style.css              → served as text/css
  _layouts/page.html     → private (used by handlers via fs.read)
  _content/article.md    → private (rendered by handlers)
```

### Templates

Tagged template literals with auto-escaping:

```rex
body = html`<h1>${title}</h1>
<div>${html.raw(markdown.render(content))}</div>`

template.render(fs.read("routes/_layouts/page.html"), {
  title: "My Page"
  body: body
})
```

The `html` tag auto-escapes interpolated values (XSS-safe). Use `html.raw()` for pre-rendered HTML.

## Configuration

Optional `rex-serve.toml` in the project root:

```toml
[server]
host = "0.0.0.0"
port = 3000
gas_limit = 1_000_000

[routes]
dir = "routes"

[db]
backend = "sqlite" # "sqlite", "upstash", or "auto"
path = "data.db"
```

`auto` uses Upstash Redis when both `UPSTASH_REDIS_REST_URL` and
`UPSTASH_REDIS_REST_TOKEN` are present, otherwise SQLite. `upstash` requires
both variables and fails startup if either is missing. The `db.*` string KV
operations use the selected backend; CAS/git object storage remains local.

Environment variables prefixed with `REX_SECRET_` are exposed read-only through
the Rex `secrets` object. For example, `REX_SECRET_API_KEY` is available as
`secrets.api-key`.

## Type Checking

Place a `.rexd` file in the project root to enable type checking. The type checker runs automatically:

- **On startup**: checks all `.rex` files against the domain schema
- **On file save**: incrementally checks only the changed files
- **Diagnostics** appear in the server log with `file:line` format

```
INFO  type checking with rex-serve.rexd
ERROR api/_middleware.rex:19: variable 'principal' is assigned but never used
INFO  type check: 1 error(s), 0 warning(s)
```

See [rex-serve.rexd](../../examples/knowledge-base/rex-serve.rexd) for a complete domain interface example.

## Live Reload

The server watches the `routes/` directory for changes:

- **`.rex` files**: recompiled, route table rebuilt, type checked
- **Static files**: route table rebuilt (content served fresh per-request)
- **Browser**: auto-reloads via WebSocket (`/__reload` endpoint)

The development loop: edit → save → server reloads + type checks → browser refreshes. Full feedback in ~100ms.

## Available Opcodes

### Request/Response

| Variable      | Type    | Description                              |
|---------------|---------|------------------------------------------|
| `method`      | string  | HTTP method                              |
| `path`        | string  | URL path                                 |
| `headers`     | map     | Request headers (case-insensitive)       |
| `query`       | map     | Query string parameters                  |
| `cookies`     | map     | Cookie values                            |
| `body`        | string  | Request body                             |
| `params`      | object  | Route parameters from `[param]` segments |
| `res.status`  | integer | Response status (default 200)            |
| `res.headers` | map     | Response headers (mutable)               |

### Functions

| Function                       | Returns     | Description                                            |
|--------------------------------|-------------|--------------------------------------------------------|
| `json.parse(text)`             | value       | Parse JSON string                                      |
| `json.stringify(value)`        | string      | Serialize to JSON                                      |
| `db.get(key)`                  | string/none | Get from SQLite KV store                               |
| `db.set(key, value)`           | boolean     | Set in KV store                                        |
| `db.delete(key)`               | boolean     | Delete from KV store                                   |
| `db.list(prefix)`              | array       | List entries by key prefix                             |
| `fs.read(path)`                | string/none | Read file (sandboxed to project root)                  |
| `fs.glob(pattern)`             | array       | List matching files                                    |
| `markdown.render(text)`        | string      | Render markdown to HTML                                |
| `template.render(tmpl, data)`  | string      | Mustache-style template substitution                   |
| `html.escape(text)`            | string      | Escape HTML entities                                   |
| `html.highlight(source)`       | string      | Syntax-highlight Rex source (Tokyo Night)              |
| `html.raw(html)`               | object      | Mark string as safe HTML (skip escaping in `html` tag) |
| `time.now()`                   | integer     | Unix timestamp (ms)                                    |
| `time.uuid()`                  | string      | Generate UUIDv7                                        |
| `crypto.hash(algo, data)`      | string      | Hash (e.g. "sha256")                                   |
| `crypto.hmac(algo, key, data)` | string      | HMAC                                                   |
| `crypto.random(bytes)`         | string      | Random hex string                                      |
| `log.info(msg)`                | none        | Log info                                               |

## WebSocket

### Live Reload

The `/__reload` endpoint broadcasts file change paths to connected browsers. The layout template includes a client script that auto-reconnects and reloads.

### Pub/Sub Channels

The `/__ws/{channel}` endpoint provides pub/sub messaging. If `routes/_ws/{channel}.rex` exists, each message is transformed through the Rex script before broadcast.

## Project Structure

```
crates/rex-serve/
  src/
    main.rs         CLI entry point
    server.rs       axum app, hot reload, WebSocket, type checking
    router.rs       filesystem scan, route table, pattern matching
    handler.rs      request → Rex execution → response
    refs.rs         HostObject implementations
    opcodes.rs      opcode implementations + syntax highlighter
    config.rs       rex-serve.toml parsing
    kv.rs           in-memory KV store with pub/sub
```

## Language Review

See [REVIEW.md](REVIEW.md) for a detailed assessment of building with Rex — what works well, what was painful, and how the language evolved during development.
