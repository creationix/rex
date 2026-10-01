// GET/PUT/DELETE: manage individual user

user-id = params.user-id

record = db.get(`user:${user-id}`)
unless record do
  status = 404
  return {ok: false, error: "user_not_found"}
end
target = json.parse(record)

when method == "GET" do
  return {
    ok: true
    id: target.id
    username: target.username
    email: target.email
    role: target.role
    created: target.created
  }
end

when method == "PUT" do
  input = json.parse(body)
  unless input do
    status = 422
    return {ok: false, error: "invalid_body"}
  end

  when input.role do
    target.role = input.role
  end
  when input.email do
    db.delete(`email:${target.email}`)
    db.set(`email:${input.email}`, json.stringify({id: target.id}))
    target.email = input.email
  end

  db.set(`user:${user-id}`, json.stringify(target))
  return {ok: true}
end

when method == "DELETE" do
  db.delete(`username:${target.username}`)
  db.delete(`email:${target.email}`)
  db.delete(`user:${user-id}`)

  // Clean up tokens
  for entry in db.list(`user-tokens:${user-id}:`) do
    token-data = json.parse(entry.value)
    db.delete(`api-token:${token-data.token}`)
    db.delete(entry.key)
  end

  return {ok: true, deleted: user-id}
end

status = 405
{ok: false, error: "method_not_allowed"}
