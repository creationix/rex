# Sample Article

This article was loaded from `_content/sample-article.md` on the filesystem.

## How It Got Here

The handler at `routes/tour/templates.rex` did three things:

1. Read this file with `fs.read("routes/_content/sample-article.md")`
2. Converted it to HTML with `markdown.render(content)`
3. Injected the HTML into `_layouts/page.html` with `template.render(layout, {body: html})`

## Markdown Features

Rex-serve uses **pulldown-cmark** for rendering. It supports:

- **Bold** and *italic*
- `inline code` and code blocks
- [Links](/) and images
- Lists (ordered and unordered)
- Blockquotes

> Like this one. Markdown makes it easy to author content without HTML.

```rex
/* You can even show Rex code in markdown */
when content do
  html = markdown.render(content)
  template.render(layout, {body: html})
end
```
