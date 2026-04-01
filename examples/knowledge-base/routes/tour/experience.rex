/* Tour Stop 6: Developer Experience Report */
res.headers.content-type = "text/html"
layout = fs.read("routes/_layouts/page.html")
unless layout do
  status = 500
  return "layout not found"
end

/* Pre-highlight all code snippets */
snippet-1 = "/* This just works. No truthiness bugs. */\napi-key = headers.authorization\n\nunless api-key do       /* only fires if truly absent */\n  res.status = 401\nend\n\nmax = query.limit or 100  /* 0 is a valid limit, won't fall through */"
snippet-2 = "items = [json.parse(a.value) for a in db.list(\"article:\")]\n{ok: true, articles: [{slug: a.slug, title: a.title} for a in items]}"
snippet-3 = "template.render(layout, {title: title, body: html})\n/*                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^\n   This object is passed as Lazy(span) to the opcode.\n   The opcode can't access the interpreter to resolve\n   the variable references inside it. */"
snippet-4 = "body = body + \"<li><a href=\\\"\" + url + \"\\\">\" + title + \"</a></li>\""
snippet-5 = "list = list + html`<li>${name}</li>`\n`<ul>${list}</ul>`"

hl-1 = html.raw(html.highlight(snippet-1))
hl-2 = html.raw(html.highlight(snippet-2))
hl-3 = html.raw(html.highlight(snippet-3))
hl-4 = html.raw(html.highlight(snippet-4))
hl-5 = html.raw(html.highlight(snippet-5))

body = html`<h1>Building rex-serve: A Developer Experience Report</h1>
<p class="source-link"><a href="/">Back to Home</a></p>

<p>This page documents the experience of building rex-serve — embedding the Rex
interpreter inside an HTTP server. What was powerful, what was surprising, and
what made things harder than expected.</p>

<hr>

<h2>What Worked Beautifully</h2>

<h3>Existence-based semantics are perfect for HTTP</h3>
<p>Rex's core insight — only <code>none</code> represents absence, while <code>false</code>,
<code>null</code>, <code>0</code>, and <code>""</code> are all real values — eliminates an entire
class of bugs in request handling. Missing headers, absent query params, and optional
fields all behave correctly without special-casing:</p>
<pre>${hl-1}</pre>
<p>In most languages you'd need <code>if api_key is not None</code> or
<code>?? default</code> operators. In Rex, <code>or</code> means exactly what you want.</p>

<h3>The HostObject trait is a great embedding API</h3>
<p>The Rust <code>HostObject</code> trait (<code>get</code>, <code>set</code>, <code>call</code>,
<code>delete</code>, <code>iter_*</code>) maps perfectly to HTTP concepts. Request headers
became a HostObject with case-insensitive <code>get()</code>. Response headers became a
mutable HostObject with <code>set()</code>. The interpreter handles property chains like
<code>res.headers.content-type = "text/html"</code> by navigating through nested
host objects — no special HTTP-aware code needed in the interpreter.</p>

<h3>Compact, self-contained programs</h3>
<p>Rex programs are refreshingly short. A complete CRUD handler for articles is about
40 lines. The middleware for auth is 15 lines. There's no boilerplate — no imports,
no class definitions, no async/await ceremony. The program is just expressions
that transform request data into a response.</p>

<h3>Comprehensions for data transformation</h3>
<p>Transforming API data is concise and readable:</p>
<pre>${hl-2}</pre>
<p>This replaces what would be <code>map()</code> chains or explicit loops in other languages.</p>

<h3>Gas-bounded execution</h3>
<p>Every Rex program runs with a gas limit. If a handler hits an infinite loop or
runaway recursion, it terminates cleanly with a <code>GasLimitExceeded</code> error
instead of hanging the server. This is critical for running user-provided code safely.</p>

<hr>

<h2>What Was Painful</h2>

<h3>Lazy evaluation of object literals (fixed in v2)</h3>
<p>This <em>was</em> the biggest obstacle. The v1 bytecode format emitted all object
literals as lazy containers — bytecode spans only evaluated on access. When passed
to opcodes, they arrived as opaque blobs the host couldn't read. The fix required
adding <code>force_value()</code> to the interpreter at multiple points.</p>
<pre>${hl-3}</pre>
<p>The v2 bytecode migration solved this properly: containers are now <strong>eager by
default</strong>. Laziness is opt-in via an explicit index marker. Object literals in
handler code evaluate immediately — no workarounds needed. This was the single
biggest improvement from the v2 migration.</p>

<h3>Pointer deduplication interacts badly with skipped branches (fixed)</h3>
<p>The interpreter had two bugs triggered by pointer dedup: object keys deduped as
pointers were misidentified as schema pointers, and navigation places deduped as
pointers silently skipped writes. Both were interpreter bugs, not encoder bugs — the
pointers were correct. Fixed with 13 regression tests.
<code>compile_no_dedup()</code> workaround removed.</p>

<h3>No early return (fixed: <code>return</code> keyword)</h3>
<p>This <em>was</em> the second biggest pain point. Without <code>return</code>, every handler
needed <code>when/else</code> chains because the last expression's value wins. The
<code>return</code> keyword now enables clean guard-style dispatch — sequential
<code>when</code> blocks with early exit. Every rex-serve handler and middleware has
been rewritten to use it.</p>

<h3>No closure or callback model</h3>
<p>Rex programs are linear scripts, not event-driven. There's no way to define a
function and call it later, or register a callback. Every middleware and handler
is a separate program with separate compilation. Variables flow between them only
because the server manually chains <code>RunResult.vars</code> into the next program's
context. This works, but it means the middleware can't define helper functions that
handlers inherit.</p>

<h3>Opcode namespace wiring</h3>
<p>The Rex compiler treats <code>time.uuid()</code> as a variable navigation:
<code>$time.uuid</code>. But opcodes are registered as short codes like <code>%tu</code>.
Bridging these required creating HostObject "namespace" objects that return opcode
strings when navigated. So <code>$time</code> is a HostObject where
<code>get("uuid")</code> returns <code>"%tu"</code>, which the interpreter then
dispatches as an opcode call. It works, but it's a layer of indirection that the
compiler could eliminate if it knew about domain functions.</p>

<h3>String concatenation for HTML (now solved)</h3>
<p>Before template literals, building HTML meant lots of string concatenation with escaped quotes:</p>
<pre>${hl-4}</pre>
<p>Template literals and tagged templates now solve this. The <code>html</code>
tag auto-escapes interpolated values, preventing XSS while keeping static HTML clean:</p>
<pre>${hl-5}</pre>
<p>Untagged backtick templates handle composition of already-safe HTML fragments.
This page's static-files tour stop demonstrates the pattern.</p>

<hr>

<h2>Architecture Insights</h2>

<h3>The interpreter is fast enough</h3>
<p>The zero-copy cursor interpreter evaluates bytecode directly without building an
AST. For typical handlers (10-50 expressions), execution takes microseconds.
SQLite I/O dominates. The <code>spawn_blocking</code> approach — running synchronous
Rex on Tokio's blocking thread pool — works well because programs are so short-lived.</p>

<h3>The type file (.rexd) is a good idea</h3>
<p>Separating the type interface from the runtime means the LSP can provide
completions and diagnostics without running the server. The <code>rex-serve.rexd</code>
file declares every opcode, global, and type — one file gives you full IDE support
for the entire server API.</p>

<h3>Filesystem routing is genuinely simple</h3>
<p>No router configuration. No decorator syntax. No manifest file. Create a file,
it becomes a route. The <code>_middleware.rex</code> convention for middleware is
immediately understandable. The <code>_</code> prefix for private directories is clean.
This is the part of the DX that feels most polished.</p>

<hr>

<h2>Verdict</h2>
<p>Rex's core language semantics — existence-based logic, unified navigation,
type predicates, comprehensions — are genuinely well-suited for edge function
scripting. The pain points are mostly in the toolchain (lazy eval, pointer dedup,
namespace wiring) rather than in the language design itself. A production version
would want the compiler to emit domain-aware bytecode that wires opcodes directly,
and an interpreter mode that evaluates opcode arguments eagerly by default.</p>`

template.render(layout, {
  title: "DX Report"
  body: body
  footer: "<a href='/tour/api'>&larr; API</a> &middot; <a href='/'>Home</a>"
})
