// Middleware provides:
extern user: some

// GET/POST/DELETE personal API tokens

when method == "GET" do
  entries = db.list(`user-tokens:${user.id}:`)
  tokens = [ json.parse(e.value) for e in entries ]
  return {ok: true, tokens: tokens}
end

when method == "POST" do
  input = json.parse(body)
  unless input and input.name do
    status = 422
    return {ok: false, error: "missing_name"}
  end

  now = time.now()
  token = crypto.random(32)

  db.set(`api-token:${token}`, json.stringify({
    user-id: user.id
    name: input.name
    created: now
  }))
  db.set(`user-tokens:${user.id}:${token}`, json.stringify({
    token: token
    name: input.name
    created: now
  }))

  return {ok: true, token: token, name: input.name}
end

when method == "DELETE" do
  input = json.parse(body)
  unless input and input.token do
    status = 422
    return {ok: false, error: "missing_token"}
  end

  // Verify token belongs to this user
  api-token = db.get(`api-token:${input.token}`)
  unless api-token do
    status = 404
    return {ok: false, error: "token_not_found"}
  end
  api-token = json.parse(api-token)
  unless api-token.user-id == user.id do
    status = 403
    return {ok: false, error: "forbidden"}
  end

  db.delete(`api-token:${input.token}`)
  db.delete(`user-tokens:${user.id}:${input.token}`)
  return {ok: true}
end

status = 405
{ok: false, error: "method_not_allowed"}
