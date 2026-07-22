// Middleware provides:
extern user: some
extern token: str | none

// GET/PUT/DELETE own account

when method == "GET" do
  return {
    ok: true
    id: user.id
    username: user.username
    email: user.email
    role: user.role
    created: user.created
  }
end

when method == "PUT" do
  input = json.parse(body)
  unless input do
    status = 422
    return {ok: false, error: "invalid_body"}
  end

  // Update email
  when input.email do
    when db.get(`email:${input.email}`) do
      status = 409
      return {ok: false, error: "email_taken"}
    end
    db.delete(`email:${user.email}`)
    db.set(`email:${input.email}`, json.stringify({id: user.id}))
    user.email = input.email
  end

  // Update password
  when input.password do
    salt = crypto.random(16)
    user.salt = salt
    user.password-hash = crypto.hash("sha256", `${input.password}${salt}`)
  end

  db.set(`user:${user.id}`, json.stringify(user))
  return {ok: true}
end

when method == "DELETE" do
  // Remove all user data
  db.delete(`username:${user.username}`)
  db.delete(`email:${user.email}`)
  db.delete(`user:${user.id}`)

  // Remove user's API tokens
  for entry in db.list(`user-tokens:${user.id}:`) do
    token-data = json.parse(entry.value)
    db.delete(`api-token:${token-data.token}`)
    db.delete(entry.key)
  end

  // Remove session
  when token do
    db.delete(`session:${token}`)
  end

  return {ok: true, deleted: user.id}
end

status = 405
{ok: false, error: "method_not_allowed"}
