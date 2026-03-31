/* Article collection: GET list, POST create */
when method == "GET" do
  articles = db.list("article:")
  items = [json.parse(a.value) for a in articles]
  {ok: true, articles: [{slug: a.slug, title: a.title, updated: a.updated} for a in items]}
else when method == "POST" do
  input = json.parse(body)

  unless input and input.slug and input.title and input.body do
    res.status = 422
    {ok: false, error: "slug_title_body_required"}
  end

  when input and input.slug and input.title and input.body do
    record = {
      slug: input.slug
      title: input.title
      body: input.body
      created: time.now()
      updated: time.now()
    }
    db.set("article:" + input.slug, json.stringify(record))
    res.status = 201
    {ok: true, slug: input.slug}
  end
else
  res.status = 405
  {ok: false, error: "method_not_allowed"}
end
