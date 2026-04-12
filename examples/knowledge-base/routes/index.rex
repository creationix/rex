/* Homepage — guided tour entry point */
res.headers.content-type = "text/html; charset=utf-8"
layout = fs.read("routes/_layouts/page.html")
unless layout do
  status = 500
  return "layout not found"
end
self-source = fs.read("routes/index.rex")
highlighted = when self-source do html.raw(html.highlight(self-source)) end

body = html`<h1>Welcome to rex-serve</h1>
<p>This is a live, self-documenting demo of <strong>rex-serve</strong> — an HTTP server
that uses <a href="https://github.com/creationix/rex">Rex</a> scripts as edge functions.
Every page you see is powered by a <code>.rex</code> file. Walk through the tour below
to understand each feature.</p>

<p style="color:var(--muted);font-size:0.9rem">Tip: add the header <code>X-View-Source: 1</code> to any
request to see the raw Rex source that generated it.
Try <code>curl -H 'X-View-Source: 1' http://localhost:4000/</code></p>

<hr>
<h2>Guided Tour</h2>
<div class="grid">

<div class="card">
<h3><span class="badge badge-green">1</span> <a href="/tour/static-files">Static Files</a></h3>
<p>CSS, images, and other assets are served directly alongside Rex handlers. This page's stylesheet is a static file.</p>
</div>

<div class="card">
<h3><span class="badge badge-green">2</span> <a href="/tour/routing">Filesystem Routing</a></h3>
<p>Files map to URLs. Dynamic segments like <code>[slug].rex</code> capture path params. This tour uses them.</p>
</div>

<div class="card">
<h3><span class="badge badge-green">3</span> <a href="/tour/middleware">Middleware</a></h3>
<p>The <code>_middleware.rex</code> convention adds behavior to entire subtrees — auth, headers, the view-source tool.</p>
</div>

<div class="card">
<h3><span class="badge badge-yellow">4</span> <a href="/tour/templates">Templates &amp; Markdown</a></h3>
<p>Rex handlers read markdown from <code>_content/</code>, render it to HTML, and inject it into layout templates.</p>
</div>

<div class="card">
<h3><span class="badge badge-yellow">5</span> <a href="/tour/api">JSON API</a></h3>
<p>A CRUD API for articles — with auth middleware, request body parsing, and a SQLite-backed KV store.</p>
</div>

<div class="card">
<h3><span class="badge badge-blue">6</span> <a href="/tour/experience">Developer Experience</a></h3>
<p>Reflections on building rex-serve: what worked well in the Rex language, and what was painful.</p>
</div>

</div>

<hr>
<h2>How It Works</h2>
<p>The file tree below is the entire application. Rex handlers (<code>.rex</code>) are compiled to bytecode
on startup and executed per-request. Private directories starting with <code>_</code> are readable by handlers
via <code>fs.read()</code> but never served directly.</p>
<pre>routes/
  _middleware.rex          # Global: security headers, view-source tool
  _layouts/page.html       # HTML template used by all pages
  _content/*.md            # Markdown articles
  style.css                # Static CSS (dark/light mode)
  index.rex                # This page
  health.rex               # JSON health check
  tour/
    static-files.rex       # Tour stop 1
    routing.rex            # Tour stop 2
    middleware.rex          # Tour stop 3
    templates.rex           # Tour stop 4
    api.rex                # Tour stop 5
    experience.rex         # Tour stop 6
  api/
    _middleware.rex         # Auth for API routes
    articles.rex            # GET list / POST create
    articles/[slug].rex     # GET / PUT / DELETE</pre>

<h2>This Page's Source</h2>
<p>The Rex program that generated this page:</p>
<pre>${highlighted}</pre>`

template.render(layout, {
  title: "Home"
  body: body
  footer: "Powered by Rex &middot; <a href='/health'>Health Check</a>"
})
