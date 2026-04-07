// POST: create session token
unless method == "POST" do
  status = 405
  return {ok: false, error: "method_not_allowed"}
end

input = json.parse(body)
unless input and input.username and input.password do
  status = 422
  return {ok: false, error: "missing_fields"}
end

// Look up user by username
lookup = db.get(`username:${input.username}`)
unless lookup do
  status = 401
  return {ok: false, error: "invalid_credentials"}
end
lookup = json.parse(lookup)

record = db.get(`user:${lookup.id}`)
unless record do
  status = 401
  return {ok: false, error: "invalid_credentials"}
end
user-data = json.parse(record)

// Verify password
hash = crypto.hash("sha256", `${input.password}${user-data.salt}`)
unless hash == user-data.password-hash do
  status = 401
  return {ok: false, error: "invalid_credentials"}
end

// Create session
now = time.now()
token = crypto.random(32)
db.set(`session:${token}`, json.stringify({
  user-id: user-data.id
  created: now
  expires: now + 86400000
}))

{ok: true, token: token, user-id: user-data.id}
