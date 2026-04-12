/* Single article: GET, PUT, DELETE */
slug = params.slug
unless slug do
  res.status = 400
  return { ok: false error: "missing_slug" }
end

when method == "GET" do
  record = db.get("article:" + slug)
  unless record do
    res.status = 404
    return { ok: false error: "not_found" }
  end
  return { ok: true article: json.parse(record) }
end

when method == "PUT" do
  input = json.parse(body)
  existing = db.get("article:" + slug)

  unless existing do
    res.status = 404
    return { ok: false error: "not_found" }
  end

  old = json.parse(existing)
  updated = {
    slug: slug
    title: input.title or old.title
    body: input.body or old.body
    created: old.created
    updated: time.now()
  }
  db.set("article:" + slug, json.stringify(updated))
  return { ok: true slug: slug }
end

when method == "DELETE" do
  db.del("article:" + slug)
  return { ok: true deleted: slug }
end

res.status = 405
{ ok: false error: "method_not_allowed" }
