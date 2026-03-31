/* Single article: GET, PUT, DELETE */
slug = params.slug

when method == "GET" do
  record = db.get("article:" + slug)
  unless record do
    res.status = 404
    {ok: false, error: "not_found"}
  end
  when record do
    {ok: true, article: json.parse(record)}
  end
else when method == "PUT" do
  input = json.parse(body)
  existing = db.get("article:" + slug)

  unless existing do
    res.status = 404
    {ok: false, error: "not_found"}
  end

  when existing do
    old = json.parse(existing)
    updated = {
      slug: slug
      title: input.title or old.title
      body: input.body or old.body
      created: old.created
      updated: time.now()
    }
    db.set("article:" + slug, json.stringify(updated))
    {ok: true, slug: slug}
  end
else when method == "DELETE" do
  db.delete("article:" + slug)
  {ok: true, deleted: slug}
else
  res.status = 405
  {ok: false, error: "method_not_allowed"}
end
