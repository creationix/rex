# Rex in Practice: Lessons from rex-serve

rex-serve embeds Rex as the scripting layer for an HTTP server — filesystem-routed `.rex` files as edge functions with middleware, templates, markdown rendering, a CRUD API, and real-time WebSocket pub/sub. This document captures what stood out during development.

## Favorite Features

### Existence semantics eliminate an entire class of bugs

HTTP handlers are full of optional values — headers that may not exist, query params that may be absent, database lookups that return nothing. In most languages, you need `!= null` or `?? default` checks everywhere and still get bitten by `0` or `""` being falsy. Rex's existence model makes `or` do exactly what you mean:

```rex
api-key = headers.authorization     /* none if missing, string if present */
max = query.limit or 100            /* 0 is a valid limit, won't fall through */

unless api-key do
  res.status = 401
  return {ok: false, error: "unauthorized"}
end
```

### Guard-style handlers with `return`

Sequential `when` blocks with `return` let you write handlers as flat guard clauses — no nesting, no `else` chains, top-to-bottom readability:

```rex
when method == "GET" do
  return {ok: true, data: db.list("items:")}
end
when method == "POST" do
  input = json.parse(body)
  return {ok: true, created: input.slug}
end
res.status = 405
{ok: false, error: "method_not_allowed"}
```

### Template literals with safe-by-default HTML

Tagged templates let hosts define domain-specific string processing. The `html` tag auto-escapes interpolated values, while `html.raw()` marks pre-rendered HTML as safe:

```rex
body = html`<h1>${title}</h1>
<div>${html.raw(markdown.render(content))}</div>
<footer>Generated at ${time.now()}</footer>`
```

Nested templates work naturally — use `html` for escaping user data, untagged backticks for composing safe fragments.

### Comprehensions + unified navigation

Mapping, filtering, and reshaping data reads like a pipeline. The `.` syntax works uniformly across headers, config, JSON, arrays, and host objects:

```rex
articles = db.list("article:")
items = [json.parse(a.value) for a in articles]
{ok: true, articles: [{slug: a.slug, title: a.title} for a in items]}
```

### Type checking catches real bugs

The type checker runs on startup and on every file save via hot reload. Running `rex check` against the `.rexd` domain interface caught genuine issues that visual testing missed — including a string escaping bug (`\\"` terminating a string early) that was only detectable because the checker flagged the resulting variable as unused. The per-field `mut` declarations precisely express which parts of the response a handler can write to:

```rex
extern "S" res: {
  mut status: int
  mut headers: {mut *: str}
}
```

### Explicit shortcodes give the compiler enough info

The `.rexd` shortcode strings (`extern "jp" json.parse(...)`) let the compiler rewrite dotted calls directly to opcodes at compile time. This eliminates the runtime namespace indirection — no more HostObjects returning `"%jp"` strings. The trade-off is manual maintenance: the shortcode must match the runtime's opcode registry exactly.

## Language Evolution During Development

Every original pain point was resolved during the project:

| Issue                                 | Resolution                                                                |
|---------------------------------------|---------------------------------------------------------------------------|
| **Lazy maps break across boundaries** | v2 bytecode: eager by default, lazy opt-in via index                      |
| **No early return**                   | `return` keyword — halts execution, propagates through all scopes         |
| **String concatenation for HTML**     | Template literals with `${expr}`, tagged templates for auto-escaping      |
| **Pointer dedup bugs**                | Interpreter fixed to handle pointers in all positions                     |
| **Runtime opcode indirection**        | `compile_with_domain` rewrites opcodes directly from `.rexd` declarations |
| **No type checking**                  | `rex check` validates against `.rexd` — integrated into hot reload        |
| **Separate when/unless bytecode**     | Unified into variadic `?` cond                                            |
| **Binary and/or**                     | Now variadic — `a and b and c` is a single `&(a b c)`                     |
| **Intersection types too complex**    | Removed `str & [str]` from `.rexd` — just `str` everywhere               |

## Current Rough Edges

### Keywords can't be method names

`db.delete(key)` doesn't compile — `delete` is a keyword. The parser accepts keywords after `.` in navigation reads, but the lowerer's Call structure doesn't match the shortcode rewrite pattern. Renamed to `db.del()`. Any host API method named after a keyword needs a workaround.

### Shortcode refs shadow mutable variables

`extern "B" body: str` rewrites every `body` reference to a read-only ref. But handler scripts routinely reassign `body` to build HTML. The ref makes the assignment silently no-op — the page renders empty with no error. Bindings that user code might shadow should not use shortcode refs.

## Status

The platform is fully functional. The type checker catches real bugs and the explicit shortcode system works. The remaining friction is lexical — keywords blocking method names, shortcode ref shadowing — rather than architectural.
