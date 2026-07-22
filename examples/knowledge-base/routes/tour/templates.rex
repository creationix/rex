/* Tour Stop 4: Templates & Markdown */
res.headers.content-type = "text/html; charset=utf-8"
layout = fs.read("routes/_layouts/page.html")
unless layout do
  status = 500
  return "layout not found"
end

/* Render the sample article to demonstrate the pipeline */
sample-md = fs.read("routes/_content/sample-article.md")
sample-html = when sample-md do html.raw(markdown.render(sample-md)) end

/* Pre-highlight sources */
pipeline-snippet = "/* The 3-step content pipeline */
layout = fs.read(\"routes/_layouts/page.html\")
content = fs.read(\"routes/_content/sample-article.md\")
html-body = markdown.render(content)
template.render(layout, {title: \"My Page\", body: html-body})"
layout-source = fs.read("routes/_layouts/page.html")
hl-layout = when layout-source do html.raw(html.highlight-html(layout-source)) end
self-source = fs.read("routes/tour/templates.rex")
hl-self = when self-source do html.raw(html.highlight(self-source)) end

body = html`<h1>Tour: Templates &amp; Markdown</h1>
<p class="source-link"><a href="/tour/api">Next: API &rarr;</a></p>

<p>Rex handlers can read files, render markdown, and inject content into HTML templates.
This is the content pipeline that powers every page in this tour.</p>

<h2>The Pipeline</h2>
<div class="card">
<p><span class="badge badge-green">1</span> <strong>fs.read</strong> — read markdown from <code>_content/</code> or templates from <code>_layouts/</code></p>
<p><span class="badge badge-yellow">2</span> <strong>markdown.render</strong> — convert markdown to HTML (via pulldown-cmark)</p>
<p><span class="badge badge-blue">3</span> <strong>template.render</strong> — inject into an HTML layout using mustache syntax</p>
</div>

<pre>${html.raw(html.highlight(pipeline-snippet))}</pre>

<h2>Template Syntax</h2>
<p>Templates use mustache-style substitution:</p>
<ul>
<li><code>{{key}}</code> — HTML-escaped value (safe for user input)</li>
<li><code>{{{key}}}</code> — raw/unescaped (for pre-rendered HTML like markdown output)</li>
</ul>
<p>The layout template for this app:</p>
<pre>${hl-layout}</pre>

<h2>Private Directories</h2>
<p>The <code>_content/</code> and <code>_layouts/</code> directories start with <code>_</code>,
so the server will never serve them directly. Try requesting
<code>/_content/sample-article.md</code> — you'll get a 404. But handlers can read
them freely via <code>fs.read()</code>.</p>

<h2>Live Example: Rendered Markdown</h2>
<p>Below is <code>_content/sample-article.md</code> rendered through the pipeline:</p>
<div class="card">${sample-html}</div>

<h2>This Page's Source</h2>
<pre>${hl-self}</pre>`

template.render(layout, {
  title:"Templates"
  body:body
  footer:"<a href='/tour/middleware'>&larr; Middleware</a> &middot; <a href='/tour/api'>API &rarr;</a>"
})
