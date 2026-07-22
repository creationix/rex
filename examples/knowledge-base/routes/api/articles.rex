/* Article collection: GET list, POST create */
when method == "GET" do
  articles = db.list("article:")
  items = [ json.parse(a.value) for a in articles ]
  return { ok: true articles: [ { slug: a.slug title: a.title updated: a.updated } for a in items ] }
end

when method == "POST" do
  input = json.parse(body)

  unless input.slug and input.title and input.body do
    res.status = 422
    return { ok: false error: "slug_title_body_required" }
  end

  record = {
    slug: input.slug
    title: input.title
    body: input.body
    created: time.now()
    updated: time.now()
  }
  db.set(`article:${input.slug}`, json.stringify(record))
  res.status = 201
  return { ok: true slug: input.slug }
end

res.status = 405
{ ok: false error: "method_not_allowed" }
