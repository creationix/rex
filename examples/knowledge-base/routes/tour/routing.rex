/* Tour Stop 2: Filesystem Routing */
res.headers.content-type = "text/html; charset=utf-8"
layout = fs.read("routes/_layouts/page.html")
unless layout do
  status = 500
  return "layout not found"
end
self-source = fs.read("routes/tour/routing.rex")
highlighted = when self-source do html.raw(html.highlight(self-source)) end

routing-snippet = "when method == \"GET\" do
  /* return data */
end

when method == \"POST\" do
  input = json.parse(body)
  /* create resource */
end"

body = html`<h1>Tour: Filesystem Routing</h1>
<p class="source-link"><a href="/tour/middleware">Next: Middleware &rarr;</a></p>

<p>In rex-serve, <strong>the filesystem IS the router</strong>. Each <code>.rex</code> file maps to a URL path
based on its location in the <code>routes/</code> directory.</p>

<h2>Mapping Rules</h2>
<div class="card">
<pre>routes/
  index.rex                &rarr; GET /
  health.rex               &rarr; GET /health
  tour/routing.rex         &rarr; GET /tour/routing       &larr; this page!
  api/articles.rex         &rarr; * /api/articles
  api/articles/[slug].rex  &rarr; * /api/articles/:slug</pre>
</div>

<h2>Dynamic Segments</h2>
<p>Files named <code>[param].rex</code> capture a path segment as a parameter.
The value is available as <code>params.param</code> inside the handler.</p>
<p>For example, <code>api/articles/[slug].rex</code> handles requests like
<code>/api/articles/hello-world</code> with <code>params.slug = "hello-world"</code>.</p>

<h2>All Methods in One File</h2>
<p>Unlike Next.js, a single <code>.rex</code> file handles all HTTP methods. You branch on
<code>method</code> inside the handler:</p>
<pre>${html.raw(html.highlight(routing-snippet))}</pre>

<h2>Route Specificity</h2>
<ol>
<li>Static segments beat dynamic: <code>/api/health</code> wins over <code>/api/[id]</code></li>
<li>Named params beat catch-all: <code>[id].rex</code> wins over <code>[...rest].rex</code></li>
<li>Longer paths beat shorter paths</li>
</ol>

<h2>Try It</h2>
<details class="try-it">
<summary>Try: View the article API with a dynamic slug</summary>
<pre>curl http://localhost:4000/api/articles/test-slug -H 'Authorization: demo'</pre>
<p>The <code>[slug].rex</code> handler receives <code>params.slug = "test-slug"</code>.</p>
</details>

<h2>This Page's Source</h2>
<pre>${highlighted}</pre>`

template.render(layout, {
  title: "Routing"
  body: body
  footer: "<a href='/tour/static-files'>&larr; Static Files</a> &middot; <a href='/tour/middleware'>Middleware &rarr;</a>"
})
