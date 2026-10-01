/* Tour Stop 1: Static Files
   This page demonstrates tagged template literals — the html tag
   auto-escapes interpolated values, preventing XSS. */
res.headers.content-type = "text/html; charset=utf-8"
layout = fs.read("routes/_layouts/page.html")
unless layout do
  status = 500
  return "layout not found"
end
self-source = fs.read("routes/tour/static-files.rex")
highlighted = when self-source do html.raw(html.highlight(self-source)) end

body = html`<h1>Tour: Static Files</h1>
<p class="source-link"><a href="/tour/routing">Next: Routing &rarr;</a></p>

<p>Any file in the <code>routes/</code> directory that isn't a <code>.rex</code> file and doesn't
start with <code>_</code> is served as a static asset with automatic content-type detection.</p>

<h2>How It Works</h2>
<div class="card">
<p><strong>This page's CSS</strong> is served from <code>routes/style.css</code>.
The server detected <code>text/css</code> from the file extension and set the content-type header.
No Rex code needed.</p>
<p>Verify: <a href="/style.css">/style.css</a></p>
</div>

<h2>Resolution Priority</h2>
<p>When a request arrives, the server checks in this order:</p>
<ol>
<li><strong><code>.rex</code> handler</strong> — highest priority (dynamic)</li>
<li><strong>Static file</strong> — exact path match</li>
<li><strong><code>index.rex</code></strong> — if the path is a directory</li>
<li><strong>404</strong></li>
</ol>

<h2>Middleware Still Runs</h2>
<p>Even for static files, the middleware chain executes. That's why <code>/style.css</code>
gets the <code>X-Request-Id</code> and <code>X-Content-Type-Options: nosniff</code> headers
added by the global middleware. Try it:</p>
<details class="try-it">
<summary>Try it: inspect static file headers</summary>
<pre>curl -I http://localhost:4000/style.css</pre>
<p>You'll see the security headers alongside the CSS content-type.</p>
</details>

<h2>Tagged Template Literals</h2>
<p>This page is written using Rex's <strong>tagged template literals</strong>.
The <code>html</code> tag auto-escapes all interpolated values, preventing XSS.
Static HTML passes through unchanged, and quotes don't need escaping.</p>`

/* Show the before/after comparison using highlighted Rex snippets */
old-way = 'body = body + "<a href=" + url + ">" + title + "</a>"'
new-way = 'body = html`<a href="${url}">${title}</a>`'
body = body + "<p>Instead of string concatenation with escaped quotes:</p>"
body = body + "<pre>" + html.highlight(old-way) + "</pre>"
body = body + "<p>You write:</p>"
body = body + "<pre>" + html.highlight(new-way) + "</pre>"

body = body + html`<h2>Private Directories</h2>
<p>Directories and files starting with <code>_</code> are private — never served to
browsers, but readable by Rex handlers via <code>fs.read()</code>. This app uses:</p>
<ul>
<li><code>_layouts/</code> — HTML templates</li>
<li><code>_content/</code> — markdown articles</li>
</ul>
<p>Try requesting <code>/_layouts/page.html</code> directly — you'll get a 404.</p>

<h2>This Page's Source</h2>
<pre>${highlighted}</pre>`

template.render(layout, {
  title: "Static Files"
  body: body
  footer: "<a href='/'>&larr; Home</a> &middot; <a href='/tour/routing'>Routing &rarr;</a>"
})
