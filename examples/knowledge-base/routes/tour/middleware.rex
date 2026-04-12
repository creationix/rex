/* Tour Stop 3: Middleware */
res.headers.content-type = "text/html; charset=utf-8"
layout = fs.read("routes/_layouts/page.html")
unless layout do
  status = 500
  return "layout not found"
end

/* Read the actual middleware source to display */
global-mw = fs.read("routes/_middleware.rex")
api-mw = fs.read("routes/api/_middleware.rex")
self-source = fs.read("routes/tour/middleware.rex")
hl-global = when global-mw do html.raw(html.highlight(global-mw)) end
hl-api = when api-mw do html.raw(html.highlight(api-mw)) end
hl-self = when self-source do html.raw(html.highlight(self-source)) end

auth-snippet = "unless api-key do
  res.status = 401
  {ok: false, error: \"unauthorized\"}
end"

body = html`<h1>Tour: Middleware</h1>
<p class="source-link"><a href="/tour/templates">Next: Templates &rarr;</a></p>

<p>Files named <code>_middleware.rex</code> run before every handler in their directory
and all subdirectories. They execute root-first (most general to most specific).</p>

<h2>Middleware Chain for This Page</h2>
<p>When you requested <code>/tour/middleware</code>, the server ran:</p>
<ol>
<li><code>routes/_middleware.rex</code> — security headers, request ID, view-source tool</li>
<li><em>Then</em> this handler (<code>routes/tour/middleware.rex</code>)</li>
</ol>
<p>Check the response headers — you'll see <code>X-Request-Id</code> and <code>X-Powered-By: rex-serve</code>
added by the global middleware.</p>

<h2>Short-Circuit</h2>
<p>If middleware sets <code>res.status</code> to 400+, the chain stops and the handler never runs.
This is how auth works:</p>
<pre>${html.raw(html.highlight(auth-snippet))}</pre>
<p>Try hitting the API without credentials: <code>curl http://localhost:4000/api/articles</code></p>

<h2>Data Passing</h2>
<p>Variables set by middleware persist into the handler. The API middleware sets
<code>principal = api-key</code>, which downstream handlers can read.</p>

<h2>View-Source: A Middleware Feature</h2>
<p>The view-source tool is implemented entirely in the global middleware. Add
<code>X-View-Source: 1</code> to any request to see the Rex source:</p>
<details class="try-it">
<summary>Try: View source for this page</summary>
<pre>curl -H 'X-View-Source: 1' http://localhost:4000/tour/middleware</pre>
</details>

<h2>Global Middleware Source</h2>
<pre>${hl-global}</pre>

<h2>API Middleware Source</h2>
<pre>${hl-api}</pre>

<h2>This Page's Source</h2>
<pre>${hl-self}</pre>`

template.render(layout, {
  title: "Middleware"
  body: body
  footer: "<a href='/tour/routing'>&larr; Routing</a> &middot; <a href='/tour/templates'>Templates &rarr;</a>"
})
