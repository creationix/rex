# Rex in Practice: Lessons from rex-serve

rex-serve embeds Rex as the scripting layer for an HTTP server — filesystem-routed `.rex` files as edge functions with middleware, templates, markdown rendering, and a CRUD API. This document captures what stood out during development.

## Favorite Features

### 1. Existence semantics eliminate an entire class of bugs

When handling HTTP requests, you constantly deal with optional values — headers that may not exist, query params that may be absent, database lookups that return nothing. In most languages, you need `!= null` or `?? default` checks everywhere and still get bitten by `0` or `""` being falsy. Rex's existence model makes `or` do exactly what you mean:

```rex
api-key = headers.authorization     /* none if missing, string if present */
max = query.limit or 100            /* 0 is a valid limit, won't fall through */
name = user.nickname or user.email  /* "" is a valid nickname */

unless api-key do
  res.status = 401                  /* only fires if truly absent */
  return {ok: false, error: "unauthorized"}
end
```

### 2. Guard-style handlers with `return`

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

### 3. Comprehensions + unified navigation

Mapping, filtering, and reshaping data reads like a pipeline. The `.` syntax works uniformly for headers, config, JSON, arrays, and host objects:

```rex
articles = db.list("article:")
items = [json.parse(a.value) for a in articles]
{ok: true, articles: [{slug: a.slug, title: a.title} for a in items]}
```

### 4. Template literals with safe-by-default HTML

Tagged templates let hosts define domain-specific string processing. The `html` tag auto-escapes interpolated values, while `html.raw()` marks pre-rendered HTML as safe:

```rex
body = html`<h1>${title}</h1>
<div>${html.raw(markdown.render(content))}</div>
<footer>Generated at ${time.now()}</footer>`
```

### 5. Domain interface files (`.rexd`)

The `type`/`extern` declaration syntax cleanly separates the host API contract from runtime code. Per-field `mut` on extern declarations precisely controls what Rex programs can write to:

```rex
extern res = {
  mut status: integer
  mut headers: {mut *: string | [string]}
  body: string          // read-only
}
```

## Language Evolution During Development

Every original pain point was resolved during the project:

| Issue | Resolution |
|---|---|
| **Lazy maps break across boundaries** | v2 bytecode: eager by default, lazy opt-in via index |
| **No early return** | `return` keyword added — halts execution, propagates through blocks/loops |
| **String concatenation for HTML** | Template literals with `${expr}` interpolation, tagged templates for `html` |
| **Pointer dedup bugs** | Interpreter fixed to handle pointers in all positions (eval_block, eval_set) |
| **`self` keyword** | Removed — loop variables via `for v in` bindings are cleaner |
| **Separate `unless` bytecode** | Unified into variadic `?` cond — `unless c do t end` compiles to `?(c no' t)` |
| **Variadic `and`/`or`** | Now variadic instead of binary — `a and b and c` is a single `&(a b c)` |

## Remaining Issues

### Namespace indirection for opcodes

The compiler treats `time.uuid()` as `$time.uuid` — variable navigation. Rex-serve creates `OpcodeNamespace` host objects that return `"%tu"` when navigated, which the interpreter then dispatches as an opcode call. With domain-aware compilation (reading `.rexd` declarations), the compiler could emit `%tu` directly — eliminating the runtime indirection.

## How the Type System Helps

The type checker (now functional via `rex check`) would catch specific bugs encountered during development:

- **The "last expression wins" bug** — unused value diagnostics when `when` blocks produce values that are discarded by subsequent expressions
- **Wrong argument types** — `template.render(layout, title)` vs `template.render(layout, {title: title})` caught by function signatures in `.rexd`
- **Property typos** — `req.headrs` flagged as unknown property with "did you mean 'headers'?" suggestion
- **Per-field mutability** — `res.body = "x"` caught as a write to a read-only field when `body` isn't declared `mut`
