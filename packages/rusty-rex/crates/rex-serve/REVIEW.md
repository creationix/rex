# Rex in Practice: Lessons from rex-serve

rex-serve embeds Rex as the scripting layer for an HTTP server — filesystem-routed `.rex` files as edge functions with middleware, templates, markdown rendering, and a CRUD API. This document captures what stood out during development.

## Three Favorite Features

### 1. Existence semantics eliminate an entire class of bugs

When handling HTTP requests, you constantly deal with optional values — headers that may not exist, query params that may be absent, database lookups that return nothing. In most languages, you need `!= null` or `?? default` checks everywhere and still get bitten by `0` or `""` being falsy. Rex's existence model makes `or` do exactly what you mean:

```rex
/* All of these keep the left value — none of them are "absent" */
api-key = headers.authorization     /* none if missing, string if present */
max = query.limit or 100            /* 0 is a valid limit, won't fall through */
name = user.nickname or user.email  /* "" is a valid nickname */

unless api-key do
  res.status = 401                  /* only fires if truly absent */
end
```

### 2. Comprehensions make data transformation effortless

Mapping, filtering, and reshaping data is the core job of API handlers. Rex comprehensions are more concise than `.map().filter()` chains and read naturally:

```rex
/* Fetch from DB, parse, reshape — one pipeline */
articles = db.list("article:")
items = [json.parse(a.value) for a in articles]
{ok: true, articles: [{slug: a.slug, title: a.title} for a in items]}
```

The `for k, v in obj` variant for objects and `for k of obj` for keys-only means you never need `Object.entries()` or `Object.keys()` — the loop form tells you what you're iterating.

### 3. Unified navigation model means one syntax for everything

Reading a header, accessing a config value, navigating a JSON response, and indexing an array all use the same `.` syntax. Dynamic keys use `.(expr)`. There's no distinction between bracket access and dot access, no special map/dictionary API:

```rex
headers.content-type                /* static key */
config.(env + "-timeout")           /* dynamic key */
users.0.name                        /* array index + property */
routes.(method + " " + path)        /* table lookup */
```

This means Rex programs read like data navigation, which is exactly what HTTP handlers are — navigate the request, transform it, produce a response.

## Pain Points and Planned Fixes

### Lazy maps break when passed across boundaries (fixed in v2)

The v1 bytecode format emitted all object literals as lazy containers. When passed to opcodes, they arrived as opaque blobs. Comprehensions like `[{slug: a.slug} for a in items]` resolved loop variables to the final value instead of each iteration's value.

The v2 bytecode migration fixed this: containers are **eager by default**, with laziness opt-in via an explicit index marker. Object literals in handler code evaluate immediately — no workarounds needed.

- [x] Bytecode v2: eager by default, lazy opt-in via index marker
- [x] `force_value()` workarounds removed from interpreter

### Pointer deduplication interacts badly with skipped branches (fixed in v2)

The v1 bytecode encoder's pointer deduplication created references across conditional branches. When a pointer's target was inside a skipped `when`/`unless` branch, the interpreter misread the bytecode. This required using `compile_no_dedup()` as a workaround.

The v2 bytecode migration fixed this — `compile()` with dedup now works correctly for all handler patterns including nested `unless` blocks inside `when` branches.

- [x] Fixed by v2 bytecode migration
- [x] `compile_no_dedup` workaround removed

### No early return means sequential blocks override each other

Rex programs are expression sequences where the last expression's value wins. This forces `when/else` chains for HTTP method dispatch instead of the more natural guard-style pattern:

```rex
/* Current: must use when/else chain so only one branch produces the final value */
when method == "GET" do
  {ok: true, data: items}
else when method == "POST" do
  {ok: true, created: id}
else
  res.status = 405
  {ok: false, error: "method_not_allowed"}
end

/* With return: guard-style, top-to-bottom, early exit */
when method == "GET" do
  return {ok: true, data: items}
end
when method == "POST" do
  return {ok: true, created: id}
end
res.status = 405
{ok: false, error: "method_not_allowed"}
```

The `return` keyword is designed in bytecode-v2 as a postfix `value;` — the `;` tag follows the return value and halts execution.

- [ ] Add `return` keyword to grammar, parser, lowering, and interpreter

### String interpolation (solved: template literals)

Previously, building HTML meant escaped-quote string concatenation. Template literals and tagged templates now solve this:

```rex
/* Before: escaped quotes everywhere */
body = body + "<li><a href=\"/articles/" + slug + "\">" + title + "</a></li>"

/* After: template literals */
body = body + `<li><a href="/articles/${slug}">${title}</a></li>`

/* Tagged template: auto-escapes interpolated values (XSS-safe) */
html`<p>${user-input}</p>`
```

Tagged templates compile to calls with separated static parts and interpolated values. Hosts register tag functions as opcodes — rex-serve's `html` tag auto-escapes interpolations. Nesting is supported: use `html` for escaping user data, untagged backticks for composing safe fragments.

- [x] Template literal syntax added to grammar (backtick-delimited, `${expr}` interpolation)
- [x] Lower to string chains for untagged, calls for tagged
- [x] Lexer: backtick token with brace-depth tracking for nested templates
- [x] rex-serve `html` tagged template with auto-escaping

## How the Type System Would Have Helped

The [type system](/rex-types.md) and [`.rexd` domain interface files](/rex-types.md#domain-interface-files-rexd) would have caught specific bugs encountered during rex-serve development:

**The "last expression wins" bug** — the type checker could warn when multiple `when` blocks at the top level all produce values, since only the last one's result is used. An "unused value" diagnostic would have caught this immediately instead of requiring debugging of empty API responses.

**Wrong argument types to opcodes** — passing a string where an object was expected (e.g., `template.render(layout, title)` instead of `template.render(layout, {title: title})`) would be caught by the function signatures in `rex-serve.rexd`.

**Property access typos** — `req.headrs` (typo) would get "Unknown property 'headrs'. Did you mean 'headers'?" since `req` has known fields in the `.rexd` declaration.

**The `{*: T}` map type producing `T | none` on lookup** is the right design. Every `headers.x-something` lookup should force a `when` check before use — the type system validates what Rex's existence semantics already encourage.
