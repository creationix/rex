/* Tour Stop 6: Developer Experience Report */
res.headers.content-type = "text/html; charset=utf-8"
layout = fs.read("routes/_layouts/page.html")
unless layout do
  status = 500
  return "layout not found"
end

/* Pre-highlight all code snippets */
snippet1 = "/* This just works. No truthiness bugs. */
api-key = headers.authorization

unless api-key do       /* only fires if truly absent */
  res.status = 401
end

max = query.limit or 100  /* 0 is a valid limit, won't fall through */"
snippet2 = "items = [json.parse(a.value) for a in db.list(\"article:\")]
{ok: true, articles: [{slug: a.slug, title: a.title} for a in items]}"
snippet3 = "template.render(layout, {title: title, body: html})
/*                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^
   This object is passed as Lazy(span) to the opcode.
   The opcode can't access the interpreter to resolve
   the variable references inside it. */"
snippet4 = 'body = body + "<li><a href=" + url + ">" + title + "</a></li>"'
snippet5 = "list = list + html`<li>${name}</li>`
`<ul>${list}</ul>`"

hl1 = html.raw(html.highlight(snippet1))
hl2 = html.raw(html.highlight(snippet2))
hl3 = html.raw(html.highlight(snippet3))
hl4 = html.raw(html.highlight(snippet4))
hl5 = html.raw(html.highlight(snippet5))

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
<pre>${hl1}</pre>
<p>In most languages you'd need <code>if api_key is not None</code> or
<code>?? default</code> operators. In Rex, <code>or</code> means exactly what you want.</p>

<h3>The HostObject trait is a great embedding API</h3>
<p>The Rust <code>HostObject</code> trait (<code>get</code>, <code>set</code>, <code>call</code>,
<code>delete</code>, <code>iter_*</code>) maps perfectly to HTTP concepts. Request headers
became a HostObject with case-insensitive <code>get()</code>. Response headers became a
mutable HostObject with <code>set()</code>. The interpreter handles property chains like
<code>res.headers.content-type = "text/html; charset=utf-8"</code> by navigating through nested
host objects — no special HTTP-aware code needed in the interpreter.</p>

<h3>Compact, self-contained programs</h3>
<p>Rex programs are refreshingly short. A complete CRUD handler for articles is about
40 lines. The middleware for auth is 15 lines. There's no boilerplate — no imports,
no class definitions, no async/await ceremony. The program is just expressions
that transform request data into a response.</p>

<h3>Comprehensions for data transformation</h3>
<p>Transforming API data is concise and readable:</p>
<pre>${hl2}</pre>
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
<pre>${hl3}</pre>
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

<h3>Opcode namespace wiring (improved: explicit shortcodes)</h3>
<p>The Rex compiler treats <code>time.uuid()</code> as a variable navigation:
<code>$time.uuid</code>. But opcodes are registered as short codes like <code>%tu</code>.
The original approach used HostObject "namespace" objects that return opcode strings
when navigated — a layer of indirection the compiler can now eliminate.</p>
<p>The <code>.rexd</code> file now supports <strong>explicit shortcode strings</strong>:
<code>extern "tu" time.uuid() -> str</code> tells the compiler to rewrite
<code>time.uuid()</code> directly to <code>%tu</code> at compile time. This bypasses
the namespace indirection entirely. The shortcodes also apply to bindings —
<code>extern "M" method: HttpMethod</code> compiles <code>method</code> to a
read-only ref <code>^M</code> instead of a variable lookup. The trade-off is that
the shortcode strings are manually maintained and must match the runtime's opcode
registry — there's no auto-derivation, so a mismatch fails silently.</p>

<h3>Keywords can't be method names</h3>
<p>Rex reserves <code>delete</code> as a keyword (unary operator). This means
<code>db.delete(key)</code> doesn't compile — the parser sees <code>delete</code> as
the start of a delete expression, not a method name. The fix was renaming to
<code>db.del(key)</code>. The parser does accept keywords after <code>.</code> in
navigation, so <code>obj.delete</code> reads fine — but calling it as a function
breaks because the call's compiled form doesn't match the shortcode rewrite pattern.
Any host API that wants a method named after a keyword needs a workaround.</p>

<h3>String concatenation for HTML (now solved)</h3>
<p>Before template literals, building HTML meant lots of string concatenation with escaped quotes:</p>
<pre>${hl4}</pre>
<p>Template literals and tagged templates now solve this. The <code>html</code>
tag auto-escapes interpolated values, preventing XSS while keeping static HTML clean:</p>
<pre>${hl5}</pre>
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
scripting. Many early pain points (lazy eval, pointer dedup, namespace wiring)
have been fixed or improved. The remaining friction is in lexical edge cases
(keywords blocking method names, identifier-digit ambiguity) and the manual
nature of shortcode maintenance. The type checker
now catches real bugs — optional access without narrowing, unused variables from
broken string escaping — which is a genuine improvement over untyped scripting.</p>`

template.render(layout, {
  title: "DX Report"
  body: body
  footer: "<a href='/tour/api'>&larr; API</a> &middot; <a href='/'>Home</a>"
})
