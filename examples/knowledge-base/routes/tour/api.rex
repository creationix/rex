/* Tour Stop 5: JSON API */
res.headers.content-type = "text/html; charset=utf-8"
layout = fs.read("routes/_layouts/page.html")
unless layout do
  status = 500
  return "layout not found"
end

/* Pre-highlight all sources */
articles-source = fs.read("routes/api/articles.rex")
slug-source = fs.read("routes/api/articles/[slug].rex")
hl-articles = when articles-source do html.raw(html.highlight(articles-source)) end
hl-slug = when slug-source do html.raw(html.highlight(slug-source)) end

api-snippet = "when method == \"GET\" do
  articles = db.list(\"article:\")
  items = [json.parse(a.value) for a in articles]
  {ok: true, articles: items}
end"

body = html`<h1>Tour: JSON API</h1>
<p class="source-link"><a href="/tour/experience">Next: DX Report &rarr;</a></p>

<p>Rex handlers that return objects or arrays get automatically serialized to JSON
with <code>content-type: application/json</code>. Combined with the KV store, this gives
you a complete CRUD API in a few lines of Rex.</p>

<h2>Endpoints</h2>
<div class="card">
<p><span class="badge badge-green">GET</span> <code>/api/articles</code> — list all articles</p>
<p><span class="badge badge-yellow">POST</span> <code>/api/articles</code> — create article</p>
<p><span class="badge badge-green">GET</span> <code>/api/articles/:slug</code> — get one article</p>
<p><span class="badge badge-yellow">PUT</span> <code>/api/articles/:slug</code> — update article</p>
<p><span class="badge badge-red">DELETE</span> <code>/api/articles/:slug</code> — delete article</p>
</div>

<p>All endpoints require an <code>Authorization</code> header (enforced by
<code>api/_middleware.rex</code>).</p>

<h2>Try It</h2>
<details class="try-it">
<summary>Step 1: Seed an API key</summary>
<pre>sqlite3 examples/knowledge-base/data.db "INSERT INTO kv VALUES('keys:demo','1')"</pre>
</details>

<details class="try-it">
<summary>Step 2: Create an article</summary>
<pre>curl -X POST http://localhost:4000/api/articles \
  -H 'Authorization: demo' \
  -d '{"slug":"hello","title":"Hello World","body":"# Hello

Created via API."}'</pre>
</details>

<details class="try-it">
<summary>Step 3: List articles</summary>
<pre>curl http://localhost:4000/api/articles -H 'Authorization: demo'</pre>
</details>

<details class="try-it">
<summary>Step 4: Try without auth (expect 401)</summary>
<pre>curl http://localhost:4000/api/articles</pre>
</details>

<h2>How It Works</h2>
<p>Rex's existence-based semantics make request handling natural. <code>when</code> checks if
a value is defined (not just truthy), so <code>when method == "GET"</code> branches cleanly.
Return an object literal and the server serializes it:</p>
<pre>${html.raw(html.highlight(api-snippet))}</pre>

<h2>Database</h2>
<p>The <code>db.*</code> opcodes provide a simple key-value store backed by SQLite.
The database file is created automatically on first run.</p>
<ul>
<li><code>db.get(key)</code> — returns the value or <code>none</code></li>
<li><code>db.set(key, value)</code> — upserts a string value</li>
<li><code>db.del(key)</code> — removes a key</li>
<li><code>db.list(prefix)</code> — returns all entries matching a prefix</li>
</ul>

<h2>articles.rex Source</h2>
<pre>${hl-articles}</pre>

<h2>articles/[slug].rex Source</h2>
<pre>${hl-slug}</pre>`

template.render(layout, {
  title: "API"
  body: body
  footer: "<a href='/tour/templates'>&larr; Templates</a> &middot; <a href='/tour/experience'>DX Report &rarr;</a>"
})
